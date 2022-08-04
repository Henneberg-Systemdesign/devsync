// Copyright (C) 2022 Jochen Henneberg <jh@henneberg-systemdesign.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::Ordering;
use std::fmt;
use std::fmt::Debug;
use std::fs;
use std::fs::DirEntry;
use std::path::PathBuf;
use std::sync::Arc;

use crossbeam::channel::Sender;
use log::trace;

use super::utils::SyncError;
use super::{stats, utils, Config};

// specials
pub mod yocto;
pub use self::yocto::Yocto;
pub mod sysroot;
pub use self::sysroot::Sysroot;

// build directories
pub mod cmake;
pub use self::cmake::Cmake;
pub mod flutter;
pub use self::flutter::Flutter;
pub mod meson;
pub use self::meson::Meson;
pub mod ninja;
pub use self::ninja::Ninja;
pub mod cargo;
pub use self::cargo::Cargo;

// repositories
pub mod git;
pub use self::git::Git;
pub mod svn;
pub use self::svn::Svn;

// plain directories
pub mod simple;
pub use self::simple::Simple;

/// Sync job for certain directory.
#[derive(Debug)]
pub struct Dir {
    /// Source path to be synced.
    pub src_path: PathBuf,
    /// Target path to be synced to.
    pub target_path: PathBuf,
    /// Sub-directories inside this directory.
    pub dirs: Vec<DirEntry>,
    /// Files inside this directory.
    pub files: Vec<DirEntry>,
    /// Extraneous directories.
    pub ex_dirs: Vec<DirEntry>,
    /// Extraneous files.
    pub ex_files: Vec<DirEntry>,
    /// The job id this directory is processed in.
    pub job: u8,
    /// The global configuration [super::Config].
    pub config: Arc<Config>,
    /// Sender channel for [stats::Stats]
    pub stats_chn: Sender<stats::Transport>,
}

impl Dir {
    /// Create new directory structure.
    pub fn new(j: u8, cfg: Arc<Config>, chn: Sender<stats::Transport>) -> Self {
        Dir {
            src_path: PathBuf::new(),
            target_path: PathBuf::new(),
            dirs: Vec::new(),
            files: Vec::new(),
            ex_dirs: Vec::new(),
            ex_files: Vec::new(),
            job: j,
            config: cfg,
            stats_chn: chn,
        }
    }

    /// Add source path.
    pub fn set_src_path(mut self, p: PathBuf) -> Self {
        self.src_path = p;
        self
    }

    /// Add target path.
    pub fn set_target_path(mut self, p: PathBuf) -> Self {
        self.target_path = p;
        self
    }

    /// Helper function for [Flavour::prepare] default implementation.
    pub fn ensure_target_path(&self) -> Result<SyncMethod, SyncError> {
        let mut m = SyncMethod::Merge;

        if self.target_path.is_file() {
            trace!("Replace file {:?} with directory", self.target_path);
            fs::remove_file(&self.target_path)?
        }

        if !self.target_path.exists() {
            trace!("Create directory {:?}", self.target_path);
            m = SyncMethod::Duplicate;
            fs::create_dir(&self.target_path)?
        }

        Ok(m)
    }

    /// Helper function for [Flavour::dup] default
    /// implementation. Splitted off for use in flavours that override
    /// the default.
    pub fn dup(&self) -> Result<(), SyncError> {
        for f in &self.files {
            if let Err(e) = utils::cp(
                &self.src_path,
                &self.target_path,
                &f.path(),
                self.config.archive,
            ) {
                self.send_error(stats::Info {
                    category: Category::Unknown,
                    name: String::new(),
                    desc: format!("Failed to duplicate file {:?} because {}", f, e),
                });
            }
        }
        Ok(())
    }

    /// Helper function for [Flavour::merge] default
    /// implementation. Splitted off for use in flavours that override
    /// the default.
    pub fn merge(&self) -> Result<(), SyncError> {
        // remove extraneous files
        for f in &self.ex_files {
            if let Err(e) = fs::remove_file(f.path().as_path()) {
                self.send_error(stats::Info {
                    category: Category::Unknown,
                    name: String::new(),
                    desc: format!("Failed to remove extraneous file {:?} because {}", f, e),
                });
            }
        }

        // now check if files have changed and update those
        for f in &self.files {
            if utils::diff(&self.src_path, &self.target_path, f) {
                trace!("File {:?} has changed", &f);
                if let Err(e) = utils::cp(
                    &self.src_path,
                    &self.target_path,
                    &f.path(),
                    self.config.archive,
                ) {
                    self.send_error(stats::Info {
                        category: Category::Unknown,
                        name: String::new(),
                        desc: format!("Failed to merge file {:?} because {}", f, e),
                    });
                }
            }
        }
        Ok(())
    }

    /// Helper function to send [stats::Command::Runtime] messages to
    /// [stats::Stats].
    pub fn send_error(&self, i: stats::Info) {
        self.stats_chn
            .send(stats::Transport {
                cmd: stats::Command::Runtime,
                val: 0,
                info: Some(i),
            })
            .expect("Failed to send error");
    }
}

/// Flavour categories.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Category {
    /// Unknown flavour.
    Unknown = 0,
    /// Special directory which does not fit into any other category.
    Special = 1,
    /// Build directory.
    Build = 30,
    /// VCS repository.
    Repository = 60,
    /// Plain directory - default.
    Plain = 100,
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Category::Unknown => f.pad("Unknown"),
            Category::Special => f.pad("Special"),
            Category::Build => f.pad("Build"),
            Category::Repository => f.pad("Repository"),
            Category::Plain => f.pad("Plain"),
        }
    }
}

impl Ord for Category {
    fn cmp(&self, other: &Self) -> Ordering {
        // make sure that the complex flavours are
        // registered first, e. g. a Yocto directory might
        // also be a Git repository but we want to detect
        // it as Yocto
        (*self as i32).cmp(&(*other as i32))
    }
}

impl PartialOrd for Category {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Method that shall be used for directory synchronization.
#[derive(Debug)]
pub enum SyncMethod {
    /// Merge changed data into exisiting backup.
    Merge,
    /// Simply duplicate, e. g. if the backup directory did not exist.
    Duplicate,
}

pub trait Flavour {
    /// Returns the additional command line options for this flavour.
    fn init_opts(_opts: &mut getopts::Options)
    where
        Self: Sized;

    /// Builds a minimum template flavour sufficient for [Self::probe].
    fn template(args: &getopts::Matches) -> Self
    where
        Self: Sized;

    /// Probe if flavour matches given directory.
    fn probe(&self, d: &Dir) -> Option<Box<dyn Flavour>>;

    /// Build flavour.
    fn build(&self) -> Box<dyn Flavour>;

    /// Set [Dir] on flavour.
    fn set_dir(&mut self, d: Dir);

    /// Get [Dir] from flavour.
    fn dir(&self) -> &Option<Dir> {
        &None
    }

    /// Get flavour name.
    fn name(&self) -> &'static str {
        "Directory"
    }

    /// Get flavour category.
    fn category(&self) -> Category {
        Category::Plain
    }

    /// Don't re-scan for sub-directories but keep this flavour.
    fn stay(&self) -> bool {
        true
    }

    /// If scan shall recurse through the subdirectories.
    fn recurse(&self) -> bool {
        true
    }

    /// If this directory shall be skipped, not sync'ed.
    fn skip(&self) -> bool {
        false
    }

    /// Prepare for backup. Default implementations simply creates the
    /// target directory.
    fn prepare(&self) -> Result<SyncMethod, SyncError> {
        if let Some(d) = self.dir() {
            d.ensure_target_path()
        } else {
            Err(SyncError::Failed(
                "Cannot prepare synchronization without directory".to_string(),
            ))
        }
    }

    /// Simply duplicate files for backup.
    fn dup(&self) -> Result<(), SyncError> {
        if let Some(d) = self.dir() {
            d.dup()
        } else {
            Err(SyncError::Failed(
                "Cannot synchronize without directory".to_string(),
            ))
        }
    }

    /// Check if file update is necessary before doing backup..
    fn merge(&self) -> Result<(), SyncError> {
        if let Some(d) = self.dir() {
            d.merge()
        } else {
            Err(SyncError::Failed(
                "Cannot synchronize without directory".to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::path::{Path, PathBuf};

    fn path() -> PathBuf {
        let mut r = PathBuf::new();
        r.push(env!("CARGO_MANIFEST_DIR"));
        r.push("tests");
        r
    }

    fn init(a: bool, d: bool) -> (Arc<Config>, stats::Stats) {
        let cfg = Arc::new(Config {
            jobs: 1,
            delete: d,
            archive: a,
            owned: false,
            ignore: vec![],
        });

        let stats = stats::Stats::default();

        (cfg, stats)
    }

    fn sample_dir(p: &Path) {
        utils::create_dir_save(p, true).expect("Failed to create path");
        utils::create_dir_save(&p.join("dir_d"), false).expect("Failed to create path");
        utils::create_dir_save(&p.join("dir_f"), false).expect("Failed to create path");
        let _ = fs::File::create(p.join("file_a"));
        let _ = fs::File::create(p.join("file_b"));
        let _ = fs::File::create(p.join("file_c"));
        let _ = fs::File::create(p.join("file_e"));
        let _ = fs::File::create(p.join("dir_d").join("file_a"));
        let _ = fs::File::create(p.join("dir_d").join("file_b"));
    }

    #[test]
    fn test_new_set_and_ensure_target_path() {
        let p = path();
        let (cfg, stats) = init(false, false);

        let d = Dir::new(0, cfg, stats.sender().clone())
            .set_src_path(p.clone())
            .set_target_path(p);
        let _ = d.ensure_target_path();
        assert!(d.target_path.exists());

        // cleanup
        let _ = fs::remove_dir_all(d.target_path);
    }

    #[test]
    fn test_dup() {
        let p = path();
        let (cfg, stats) = init(false, false);

        let sp = p.join("dup_1");
        let tp = p.join("dup_2");

        sample_dir(&sp);

        let mut d = Dir::new(0, cfg.clone(), stats.sender().clone())
            .set_src_path(sp)
            .set_target_path(tp.clone());

        let _ = utils::save_dirs_and_files(
            d.src_path.as_path(),
            &mut d.dirs,
            &mut d.files,
            None,
            cfg.owned,
        );

        let _ = d.ensure_target_path();
        let _ = d.dup();

        let mut count = 0;
        for f in fs::read_dir(tp).unwrap() {
            let ff = f.unwrap();
            let t = ff.file_type().unwrap();
            count += 1;
            match ff.file_name().into_string().unwrap().as_str() {
                "file_a" | "file_b" | "file_c" | "file_e" => assert!(t.is_file()),
                _ => unreachable!(),
            }
        }
        assert!(count == 4);

        // cleanup
        let _ = fs::remove_dir_all(d.src_path);
        let _ = fs::remove_dir_all(d.target_path);
    }

    #[test]
    fn test_merge() {
        let p = path();
        let (cfg, stats) = init(false, false);

        let sp = p.join("merge_1");
        let tp = p.join("merge_2");

        sample_dir(&sp);
        // ensure timestamps differs
        std::thread::sleep(std::time::Duration::new(1, 0));
        sample_dir(&tp);
        let _ = fs::remove_file(tp.join("file_a"));

        let mut d = Dir::new(0, cfg.clone(), stats.sender().clone())
            .set_src_path(sp.clone())
            .set_target_path(tp.clone());

        let _ = utils::save_dirs_and_files(
            d.src_path.as_path(),
            &mut d.dirs,
            &mut d.files,
            None,
            cfg.owned,
        );

        let _ = d.ensure_target_path();
        let _ = d.merge();

        let mut count = 0;
        for f in fs::read_dir(&tp).unwrap() {
            let ff = f.unwrap();
            let t = ff.file_type().unwrap();
            count += 1;
            match ff.file_name().into_string().unwrap().as_str() {
                "file_a" | "file_b" | "file_c" | "file_e" => {
                    assert!(utils::diff(&tp, &sp, &ff));
                    assert!(t.is_file());
                }
                "dir_d" | "dir_f" => assert!(t.is_dir()),
                _ => unreachable!(),
            }
        }
        assert!(count == 6);

        // cleanup
        let _ = fs::remove_dir_all(d.src_path);
        let _ = fs::remove_dir_all(d.target_path);
    }

    #[test]
    fn test_merge_archive() {
        let p = path();
        let (cfg, stats) = init(true, false);

        let sp = p.join("merge_archive_1");
        let tp = p.join("merge_archive_2");

        sample_dir(&sp);
        // ensure timestamps differs
        std::thread::sleep(std::time::Duration::new(1, 0));
        sample_dir(&tp);
        let _ = fs::remove_file(tp.join("file_a"));

        let mut d = Dir::new(0, cfg.clone(), stats.sender().clone())
            .set_src_path(sp.clone())
            .set_target_path(tp.clone());

        let _ = utils::save_dirs_and_files(
            d.src_path.as_path(),
            &mut d.dirs,
            &mut d.files,
            None,
            cfg.owned,
        );

        let _ = d.ensure_target_path();
        let _ = d.merge();

        let mut count = 0;
        for f in fs::read_dir(&tp).unwrap() {
            let ff = f.unwrap();
            let t = ff.file_type().unwrap();
            count += 1;
            match ff.file_name().into_string().unwrap().as_str() {
                "file_a" | "file_b" | "file_c" | "file_e" => {
                    assert!(!utils::diff(&tp, &sp, &ff));
                    assert!(t.is_file());
                }
                "dir_d" | "dir_f" => assert!(t.is_dir()),
                _ => unreachable!(),
            }
        }
        assert!(count == 6);

        // cleanup
        let _ = fs::remove_dir_all(d.src_path);
        let _ = fs::remove_dir_all(d.target_path);
    }

    #[test]
    fn test_merge_delete() {
        let p = path();
        let (cfg, stats) = init(false, true);

        let sp = p.join("merge_delete_1");
        let tp = p.join("merge_delete_2");

        sample_dir(&sp);
        let _ = fs::remove_file(sp.join("file_a"));
        // ensure timestamps differs
        std::thread::sleep(std::time::Duration::new(1, 0));
        sample_dir(&tp);

        let mut d = Dir::new(0, cfg.clone(), stats.sender().clone())
            .set_src_path(sp)
            .set_target_path(tp.clone());

        let _ = utils::save_dirs_and_files(
            d.src_path.as_path(),
            &mut d.dirs,
            &mut d.files,
            None,
            cfg.owned,
        );

        let _ = utils::save_dirs_and_files(
            d.target_path.as_path(),
            &mut d.ex_dirs,
            &mut d.ex_files,
            None,
            cfg.owned,
        );
        utils::filter_dir_entries(&d.dirs, &mut d.ex_dirs);
        utils::filter_dir_entries(&d.files, &mut d.ex_files);

        let _ = d.ensure_target_path();
        let _ = d.merge();

        let mut count = 0;
        for f in fs::read_dir(&tp).unwrap() {
            let ff = f.unwrap();
            let t = ff.file_type().unwrap();
            count += 1;
            match ff.file_name().into_string().unwrap().as_str() {
                "file_b" | "file_c" | "file_e" => assert!(t.is_file()),
                "dir_d" | "dir_f" => assert!(t.is_dir()),
                _ => unreachable!(),
            }
        }
        assert!(count == 5);

        // cleanup
        let _ = fs::remove_dir_all(d.src_path);
        let _ = fs::remove_dir_all(d.target_path);
    }
}
