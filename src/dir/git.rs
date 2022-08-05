// Copyright (C) 2022 Jochen Henneberg <jh@henneberg-systemdesign.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs;
use std::path::Path;

use git2::build::{CloneLocal, RepoBuilder};
use git2::{
    Branch, BranchType, Delta, Email, EmailCreateOptions, ObjectType, Repository, Signature,
};
use log::trace;

use super::utils::SyncError;
use super::{stats, utils, Category, Dir, Flavour};

pub struct Git {
    dir: Box<Option<Dir>>,
    ignore: bool,
    full: bool,
    ignore_stashes: bool,
    ignore_untracked: bool,
    ignore_unstaged: bool,
    ignore_unpushed: bool,
}

impl Git {
    fn dir_unchecked(&self) -> &Dir {
        match self.dir.as_ref() {
            Some(d) => d,
            None => panic!("Flavours 'dir' entry is None"),
        }
    }

    fn subdir_create(&self, n: &str) -> Result<(), SyncError> {
        let d = self.dir_unchecked();
        let p = &d.target_path.as_path().join(n);
        utils::create_dir_save(p, true)?;
        Ok(())
    }

    fn subdir_rename(&self, n: &str, s: &str) -> Result<(), SyncError> {
        let d = self.dir_unchecked();
        let p = &d.target_path.as_path().join(n);
        if p.exists() {
            fs::remove_dir_all(p)?;
        }
        fs::File::create(&d.target_path.as_path().join(&format!("{}.{}", n, s)))?;
        Ok(())
    }

    fn subdir_ignored(&self, n: &str) -> Result<(), SyncError> {
        self.subdir_rename(n, "ignored")
    }

    fn subdir_empty(&self, n: &str) -> Result<(), SyncError> {
        self.subdir_rename(n, "empty")
    }

    /// Copy stashes to backup directory unless --git-ignore-stashes
    /// is set.
    fn dup_stashes(&self) -> Result<(), SyncError> {
        // format and cleanup previous stashes
        let d = self.dir_unchecked();
        let p = &d.target_path.as_path().join("stashes");
        self.subdir_create("stashes")?;

        if self.ignore_stashes {
            self.subdir_ignored("stashes")?;
            return Ok(());
        }

        let mut r = Repository::open(&d.src_path)?;

        let mut stashes: Vec<(git2::Oid, String)> = Vec::new();
        r.stash_foreach(|_, name, id| {
            trace!("Backup stash named {}", name);
            stashes.push((*id, name.to_string()));
            true
        })?;

        if stashes.is_empty() {
            self.subdir_empty("stashes")?;
            return Ok(());
        }

        // write one file per stash
        for (id, name) in stashes {
            let c = r
                .find_object(id, Some(ObjectType::Commit))?
                .into_commit()
                .unwrap();
            let c_p = r
                .find_object(c.parent_id(0)?, Some(ObjectType::Commit))?
                .into_commit()
                .unwrap();
            let d = r.diff_tree_to_tree(Some(&c.tree()?), Some(&c_p.tree()?), None)?;
            let sig = r.signature().or_else(|_| Signature::now("", ""));
            let mail = Email::from_diff(
                &d,
                1,
                1,
                &id,
                &name,
                &String::new(),
                &sig?,
                &mut EmailCreateOptions::default(),
            )?;
            let _ = fs::write(p.join(format!("{}-{}", name, id)), mail.as_slice());
        }
        Ok(())
    }

    /// Copy untracked/unstaged files to backup directory unless
    /// --git-ignore-untracked or --git-ignore-unstaged are set.
    fn dup_status(&self) -> Result<(), SyncError> {
        let d = self.dir_unchecked();
        let repo = Repository::open(&d.src_path)?;
        let mut r = Ok(());
        let mut empty = (true, true); // untracked / unstaged

        let tp_untracked = Path::new(&d.target_path).join("untracked");
        if self.ignore_untracked {
            self.subdir_ignored("untracked")?;
            empty.0 = false;
        } else {
            self.subdir_create("untracked")?;
        }

        let tp_unstaged = Path::new(&d.target_path).join("unstaged");
        if self.ignore_unstaged {
            self.subdir_ignored("unstaged")?;
            empty.1 = false;
        } else {
            self.subdir_create("unstaged")?;
        }

        for s in repo.statuses(None)?.iter() {
            if let Some(diff) = s.index_to_workdir() {
                let p = Path::new(diff.new_file().path().unwrap());
                match diff.status() {
                    Delta::Modified if !self.ignore_unstaged => {
                        trace!("Backup unstaged {:?}", p);
                        if let Err(e) = utils::cp_r_d(&d.src_path, &tp_unstaged, p, true) {
                            d.send_runtime(stats::Info {
                                category: self.category(),
                                name: String::from(self.name()),
                                desc: format!(
                                    "Failed to backup unstaged file {:?} because {}",
                                    p, e
                                ),
                            });
                            if r.is_ok() {
                                r = Err(SyncError::Failed(format!(
                                    "Failed to backup file(s) from {:?}",
                                    d.src_path
                                )))
                            }
                        } else {
                            empty.1 = false;
                        }
                    }
                    Delta::Untracked if !self.ignore_untracked => {
                        trace!("Backup untracked {:?}", p);
                        if let Err(e) = utils::cp_r_d(&d.src_path, &tp_untracked, p, true) {
                            d.send_runtime(stats::Info {
                                category: self.category(),
                                name: String::from(self.name()),
                                desc: format!(
                                    "Failed to backup untracked file {:?} because {}",
                                    p, e
                                ),
                            });
                            if r.is_ok() {
                                r = Err(SyncError::Failed(format!(
                                    "Failed to backup file(s) from {:?}",
                                    d.src_path
                                )))
                            }
                        } else {
                            empty.0 = false;
                        }
                    }
                    _ => (),
                }
            }
        }

        if empty.0 {
            self.subdir_empty("untracked")?
        }

        if empty.1 {
            self.subdir_empty("unstaged")?
        }

        r
    }

    /// Does local branch has an upstream branch?
    fn branch_upstream(&self, b: &Branch) -> bool {
        let id_new = b.get().target().unwrap();
        let id_old = match b.upstream() {
            Ok(ub) => ub.get().target().unwrap(),
            Err(_) => return false,
        };

        id_old == id_new
    }

    /// Dupliate repository (bare) in case that there are local
    /// branches without upstream branch or if the local and upstream
    /// branch do not match.
    fn dup_repo(&self, r: &Repository) -> Result<(), SyncError> {
        let d = self.dir_unchecked();
        let p = &d.target_path.as_path().join("repo");
        let rp = r.path().parent().unwrap().to_str().unwrap();
        match RepoBuilder::new()
            .bare(true)
            .clone_local(CloneLocal::Local)
            .clone(rp, p)
        {
            Ok(_) => Ok(()),
            Err(_) => Err(SyncError::Failed(format!("Cannot clone repository {}", rp))),
        }
    }

    /// Check if bare repository clone is required.
    fn dup_local(&self) -> Result<(), SyncError> {
        self.subdir_create("repo")?;

        if self.ignore_unpushed {
            self.subdir_ignored("repo")?;
            return Ok(());
        }

        let d = self.dir_unchecked();
        let r = Repository::open(&d.src_path)?;
        let mut upd = false;
        for wb in r.branches(None)? {
            let b = wb.unwrap();
            trace!("Check branch {} upstream", b.0.name()?.unwrap());

            if b.1 == BranchType::Local && !self.branch_upstream(&b.0) {
                trace!("Branch {} not upstream", b.0.name()?.unwrap());
                upd = true;
                break;
            }
        }

        if upd {
            trace!("Backup repository");
            self.dup_repo(&r)?;
        } else {
            self.subdir_empty("repo")?;
        }

        Ok(())
    }

    /// Run all duplicate setps.
    fn dup_all(&self) -> Result<(), SyncError> {
        if let Some(d) = self.dir() {
            utils::rm_dirs_and_files(d.target_path.as_path())?;

            if let Err(e) = self.dup_stashes() {
                d.send_runtime(stats::Info {
                    category: self.category(),
                    name: self.name().to_string(),
                    desc: format!("Failed to backup stashes because {}", e),
                });
            }

            if let Err(e) = self.dup_status() {
                d.send_runtime(stats::Info {
                    category: self.category(),
                    name: self.name().to_string(),
                    desc: format!("Failed to backup status because {}", e),
                });
            }

            if let Err(e) = self.dup_local() {
                d.send_runtime(stats::Info {
                    category: self.category(),
                    name: self.name().to_string(),
                    desc: format!("Failed to backup locals because {}", e),
                });
            }
            Ok(())
        } else {
            Err(SyncError::Failed(
                "Cannot synchronize without directory".to_string(),
            ))
        }
    }
}

impl Flavour for Git {
    fn init_opts(opts: &mut getopts::Options) {
        opts.optflag("", "git-ignore", "Ignore Git repositories");
        opts.optflag("", "git-full", "Full backup (default is stashes and diff)");
        opts.optflag("", "git-ignore-stashes", "Don't backup stashes");
        opts.optflag("", "git-ignore-unstaged", "Don't backup unstaged files");
        opts.optflag("", "git-ignore-untracked", "Don't backup untracked files");
        opts.optflag("", "git-ignore-unpushed", "Don't backup unpushed branches");
    }

    fn template(args: &getopts::Matches) -> Self {
        Git {
            dir: Box::new(None),
            ignore: args.opt_present("git-ignore"),
            full: args.opt_present("git-full"),
            ignore_stashes: args.opt_present("git-ignore-stashes"),
            ignore_unstaged: args.opt_present("git-ignore-unstaged"),
            ignore_untracked: args.opt_present("git-ignore-untracked"),
            ignore_unpushed: args.opt_present("git-ignore-unpushed"),
        }
    }

    /// Probe for '.git' file to identify Git repository.
    fn probe(&self, d: &Dir) -> Option<Box<dyn Flavour>> {
        for d in &d.dirs {
            if d.file_name() == ".git" {
                return Some(self.build());
            }
        }
        None
    }

    fn build(&self) -> Box<dyn Flavour> {
        Box::new(Git {
            dir: Box::new(None),
            ignore: self.ignore,
            full: self.full,
            ignore_stashes: self.ignore_stashes,
            ignore_unstaged: self.ignore_unstaged,
            ignore_untracked: self.ignore_untracked,
            ignore_unpushed: self.ignore_unpushed,
        })
    }

    fn set_dir(&mut self, d: Dir) {
        self.dir = Box::new(Some(d));
    }

    fn dir(&self) -> &Option<Dir> {
        &*self.dir
    }

    fn name(&self) -> &'static str {
        "Git"
    }

    fn category(&self) -> Category {
        Category::Repository
    }

    /// Recurse if --git-full is set.
    fn recurse(&self) -> bool {
        self.full
    }

    /// Skip if --git-ignore is set.
    fn skip(&self) -> bool {
        self.ignore
    }

    fn stay(&self) -> bool {
        false
    }

    fn dup(&self) -> Result<(), SyncError> {
        if !self.full {
            self.dup_all()
        } else if let Some(d) = self.dir() {
            d.dup()
        } else {
            Err(SyncError::Failed(
                "Cannot synchronize without directory".to_string(),
            ))
        }
    }

    fn merge(&self) -> Result<(), SyncError> {
        if !self.full {
            self.dup_all()
        } else if let Some(d) = self.dir() {
            d.merge()
        } else {
            Err(SyncError::Failed(
                "Cannot synchronize without directory".to_string(),
            ))
        }
    }
}
