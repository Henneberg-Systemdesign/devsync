// Copyright (C) 2022 Jochen Henneberg <jh@henneberg-systemdesign.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crossbeam::thread;
use log::{error, trace};
use xml::name::OwnedName;
use xml::reader::XmlEvent;
use xml::EventReader;

use super::utils::SyncError;
use super::{utils, Category, Dir, Flavour, SyncMethod};

pub struct Svn {
    dir: Box<Option<Dir>>,
    ignore: bool,
    full: bool,
    ignore_unversioned: bool,
    ignore_modified: bool,
    modified: Vec<PathBuf>,
    unversioned: Vec<PathBuf>,
    probed: bool,
}

enum Reason {
    None,
    Modified,
    Unversioned,
}

impl From<String> for Reason {
    fn from(s: String) -> Self {
        match s.as_str() {
            "modified" => Reason::Modified,
            "unversioned" => Reason::Unversioned,
            _ => Reason::None,
        }
    }
}

impl Svn {
    fn dir_unchecked(&self) -> &Dir {
        match self.dir.as_ref() {
            Some(d) => d,
            None => panic!("Flavours 'dir' entry is None"),
        }
    }

    fn dir_unchecked_mut(&mut self) -> &mut Dir {
        match self.dir.as_mut() {
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

    fn modify_target_path(&mut self) -> Result<(), SyncError> {
        let d = self.dir_unchecked_mut();
        let mut p = d.src_path.clone();

        loop {
            let mut svn = Command::new("svn")
                .arg("info")
                .arg("--xml")
                .arg(&p)
                .stdout(Stdio::piped())
                .spawn()?;

            // our xml parser
            let mut reader = EventReader::new(svn.stdout.as_mut().unwrap());
            let mut parse = false;

            loop {
                if let Ok(e) = reader.next() {
                    match e {
                        XmlEvent::StartElement {
                            name: OwnedName { local_name: n, .. },
                            ..
                        } => parse = n.as_str() == "wcroot-abspath",
                        XmlEvent::Characters(s) if parse => {
                            let pp = d.src_path.strip_prefix(&s).unwrap();
                            for _ in pp {
                                d.target_path.pop();
                            }
                            d.target_path.push("unversioned");
                            d.target_path.push(pp);
                            break;
                        }
                        XmlEvent::EndDocument => break,
                        _ => (),
                    }
                }
            }

            match svn.wait() {
                Ok(r) => {
                    if r.success() {
                        break;
                    } else {
                        p = p.parent().unwrap().to_path_buf();
                    }
                }
                Err(_) => error!("Failed to find SVN root for {:?}", p),
            }
        }

        Ok(())
    }

    fn prepare_contents(&mut self) -> Result<(), SyncError> {
        let svn = {
            let d = self.dir_unchecked_mut();
            d.dirs.clear();
            d.files.clear();
            d.ex_dirs.clear();
            d.ex_files.clear();
            utils::rm_dirs_and_files(d.target_path.as_path())?;

            Command::new("svn")
                .arg("status")
                .arg("--xml")
                .arg(d.src_path.as_path())
                .stdout(Stdio::piped())
                .spawn()?
        };

        // our xml parser
        let mut reader = EventReader::new(svn.stdout.unwrap());
        // remember the file to handle, the file path as well as
        // modified|unversioned
        let mut file: (Option<PathBuf>, Option<Reason>) = (None, None);

        // reader thread for svn command output
        thread::scope(|scope| {
            scope.spawn(|_| {
                //let mut unversioned_dirs = vec![];
                loop {
                    if let Ok(e) = reader.next() {
                        match e {
                            XmlEvent::StartElement {
                                name: OwnedName { local_name: n, .. },
                                attributes: atts,
                                ..
                            } => match n.as_str() {
                                "entry" => {
                                    file.0 = atts.iter().find_map(|a| {
                                        (a.name.local_name == "path")
                                            .then_some(PathBuf::from(&a.value))
                                    })
                                }
                                "wc-status" => {
                                    file.1 = atts.iter().find_map(|a| {
                                        (a.name.local_name == "item")
                                            .then_some(a.value.clone().into())
                                    })
                                }
                                _ => (),
                            },
                            XmlEvent::EndDocument => break,
                            _ => (),
                        }

                        match &file {
                            (Some(f), Some(Reason::Modified)) => {
                                if !self.ignore_modified && Path::new(f).is_file() {
                                    self.modified.push(f.clone());
                                }
                                file = (None, None);
                            }
                            (Some(f), Some(Reason::Unversioned)) => {
                                if !self.ignore_unversioned {
                                    let p = Path::new(f);
                                    if p.is_dir() {
                                        for e in
                                            fs::read_dir(p.parent().unwrap()).unwrap().flatten()
                                        {
                                            if e.path().as_path() != p {
                                                continue;
                                            }
                                            if !e.file_type().unwrap().is_dir() {
                                                continue;
                                            }
                                            let dirs = &mut self.dir_unchecked_mut().dirs;
                                            dirs.push(e);
                                        }
                                    } else {
                                        self.unversioned.push(f.clone());
                                    }
                                }
                                file = (None, None);
                            }
                            (_, Some(Reason::None)) => file = (None, None),
                            _ => (),
                        }
                    }
                }
            });
        })
        .unwrap();

        Ok(())
    }

    fn dup_all(&self) -> Result<(), SyncError> {
        if let Some(d) = self.dir() {
            utils::rm_dirs_and_files(d.target_path.as_path())?;

            self.subdir_create("modified")?;
            if self.ignore_modified {
                self.subdir_ignored("modified")?;
            } else if self.modified.is_empty() {
                self.subdir_empty("modified")?;
            } else {
                for f in &self.modified {
                    trace!("Backup modified {:?}", f);
                    utils::cp_d(
                        d.src_path.as_path(),
                        &d.target_path.as_path().join("modified"),
                        f,
                        true,
                    )?;
                }
            }

            self.subdir_create("unversioned")?;
            if self.ignore_unversioned {
                self.subdir_ignored("unversioned")?;
            } else if self.unversioned.is_empty() && d.dirs.is_empty() {
                self.subdir_empty("unversioned")?;
            } else {
                for f in &self.unversioned {
                    trace!("Backup unversioned {:?}", f);
                    utils::cp_d(
                        d.src_path.as_path(),
                        &d.target_path.as_path().join("unversioned"),
                        f,
                        true,
                    )?;
                }
            }

            Ok(())
        } else {
            Err(SyncError::Failed(
                "Cannot synchronize without directory".to_string(),
            ))
        }
    }
}

impl Flavour for Svn {
    fn init_opts(opts: &mut getopts::Options) {
        opts.optflag("", "svn-ignore", "Ignore Svn repositories");
        opts.optflag(
            "",
            "svn-full",
            "Full backup (default is unversioned and modified)",
        );
        opts.optflag(
            "",
            "svn-ignore-unversioned",
            "Don't backup unversioned files",
        );
        opts.optflag("", "svn-ignore-modified", "Don't backup modified files");
    }

    fn template(args: &getopts::Matches) -> Self {
        Svn {
            dir: Box::new(None),
            ignore: args.opt_present("svn-ignore"),
            full: args.opt_present("svn-full"),
            ignore_unversioned: args.opt_present("svn-ignore-unversioned"),
            ignore_modified: args.opt_present("svn-ignore-modified"),
            modified: vec![],
            unversioned: vec![],
            probed: false,
        }
    }

    /// Look for file '.svn' to identify Subversion directory.
    fn probe(&self, d: &Dir) -> Option<Box<dyn Flavour>> {
        for d in &d.dirs {
            if d.file_name() == ".svn" {
                return Some(Box::new(Svn {
                    dir: Box::new(None),
                    ignore: self.ignore,
                    full: self.full,
                    ignore_unversioned: self.ignore_unversioned,
                    ignore_modified: self.ignore_modified,
                    modified: vec![],
                    unversioned: vec![],
                    probed: true,
                }));
            }
        }
        None
    }

    fn build(&self) -> Box<dyn Flavour> {
        Box::new(Svn {
            dir: Box::new(None),
            ignore: self.ignore,
            full: self.full,
            ignore_unversioned: self.ignore_unversioned,
            ignore_modified: self.ignore_modified,
            modified: vec![],
            unversioned: vec![],
            probed: false,
        })
    }

    fn set_dir(&mut self, d: Dir) {
        self.dir = Box::new(Some(d));
    }

    fn dir(&self) -> &Option<Dir> {
        &*self.dir
    }

    fn name(&self) -> &'static str {
        "Subversion"
    }

    fn category(&self) -> Category {
        Category::Repository
    }

    /// Recurse if --svn-ignore is not set.
    fn recurse(&self) -> bool {
        !self.ignore
    }

    /// Recurse if --svn-ignore is set.
    fn skip(&self) -> bool {
        self.ignore
    }

    fn stay(&self) -> bool {
        !self.full
    }

    /// Prepare for backup. Default implementations simply creates the
    /// target directory.
    fn prepare(&mut self) -> Result<SyncMethod, SyncError> {
        if self.dir().is_some() {
            if !self.full {
                if self.probed {
                    self.prepare_contents()?;
                } else {
                    self.modify_target_path()?;
                }
            }
            let m = self.dir_unchecked().ensure_target_path()?;
            Ok(m)
        } else {
            Err(SyncError::Failed(
                "Cannot prepare synchronization without directory".to_string(),
            ))
        }
    }

    fn dup(&self) -> Result<(), SyncError> {
        if !self.full && self.probed {
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
        if !self.full && self.probed {
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
