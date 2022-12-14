// Copyright (C) 2022 Jochen Henneberg <jh@henneberg-systemdesign.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::vec::Vec;

use crossbeam::channel::{unbounded, Receiver, Sender};
use log::{error, trace};

use super::dir::SyncMethod;
use super::utils::SyncError;
use super::{dir, stats, utils, Config};

/// Housekeeping for directory scan and processing, this object is
/// shared among all scan jobs.
type Transport = (PathBuf, Option<String>);
type Work = Box<dyn dir::Flavour + Send + Sync>;
pub struct Scan {
    /// The source path for the backup.
    pub src_path: PathBuf,
    /// The target path for the backup.
    pub target_path: PathBuf,
    /// Sender and receiver channel for new directories.
    pub scan_chn: (Sender<Transport>, Receiver<Transport>),
    /// Sender and receiver channel for directories to process.
    pub proc_chn: (Sender<Work>, Receiver<Work>),
    /// The shared scanned stats from [stats::Stats].
    scanned: Arc<Mutex<bool>>,
    /// The global configuration.
    config: Arc<Config>,
    /// The sender channel for [stats::Stats] updates.
    stats_chn: Sender<stats::Transport>,
    /// List of supported flavours.
    flavours: Vec<Work>,
}

impl Scan {
    /// Create new scan object.
    pub fn new(src: &Path, target: &Path, stats: &stats::Stats, cfg: Arc<Config>) -> Self {
        Self {
            config: cfg,
            src_path: src.to_path_buf(),
            target_path: target.to_path_buf(),
            stats_chn: stats.sender().to_owned(),
            scanned: stats.scan_done.clone(),
            scan_chn: unbounded::<(PathBuf, Option<String>)>(),
            proc_chn: unbounded::<Work>(),
            flavours: Vec::new(),
        }
    }

    /// Register template object of flavour.
    pub fn register(mut self, c: Box<dyn dir::Flavour + Send + Sync>) -> Self {
        self.flavours.push(c);

        // ensure correct order of flavours
        self.flavours.sort_unstable_by_key(|k| k.category());
        self
    }

    /// Process directory.
    pub fn scan(&self, p: &Path, f_name: Option<String>, job: u8) -> Result<(), SyncError> {
        let rp = p.strip_prefix(self.src_path.as_path()).unwrap();
        let t = self.target_path.as_path().join(rp);

        let mut d = dir::Dir::new(job, self.config.clone(), self.stats_chn.clone())
            .set_src_path(p.to_path_buf())
            .set_target_path(t);

        trace!("Remember files and directories in {:?}", p);
        utils::save_dirs_and_files(
            d.src_path.as_path(),
            &mut d.dirs,
            &mut d.files,
            Some(&self.config.ignore),
            self.config.owned,
        )?;

        // if we shall remove extraneous files and directories find
        // out which
        if self.config.delete
            && utils::save_dirs_and_files(
                d.target_path.as_path(),
                &mut d.ex_dirs,
                &mut d.ex_files,
                None,
                false,
            )
            .is_ok()
        {
            utils::filter_dir_entries(&d.dirs, &mut d.ex_dirs);
            utils::filter_dir_entries(&d.files, &mut d.ex_files);
        }

        let mut flav = self
            .flavours
            .iter()
            .find_map(|f| {
                if f_name.is_some() {
                    if f_name.as_ref().unwrap() == f.name() {
                        Some(f.build())
                    } else {
                        None
                    }
                } else if let Some(flav) = f.probe(&d) {
                    Some(flav)
                } else {
                    None
                }
            })
            .unwrap();

        if flav.skip() {
            self.send_log(stats::Info {
                category: flav.category(),
                name: flav.name().to_string(),
                desc: format!("Skipped {:?}", p),
            });
            // if we shall skip and extraneous directories shall be
            // removed do that
            if self.config.delete && d.target_path.exists() {
                fs::remove_dir_all(d.target_path.as_path())?;
            }
            self.skip_one();
            return Ok(());
        }

        // give the directory to the flavour
        flav.set_dir(d);

        match flav.prepare() {
            Ok(()) => {
                let p = flav.dir().as_ref().unwrap().src_path.as_path();
                // now tell the thread pool about new work
                if flav.recurse() {
                    let d = &mut flav.dir().as_ref().unwrap();
                    // remove extraneous directories (if set)
                    for e in &d.ex_dirs {
                        fs::remove_dir_all(e)?;
                    }
                    // send all directory entries to thread pool
                    let stay = flav.stay().then_some(flav.name().to_string());
                    for p in &d.dirs {
                        self.todo_one();
                        self.scan_chn.0.send((p.clone(), stay.clone())).unwrap();
                    }
                } else {
                    trace!("Don't scan {:?} recursively", p);
                }

                // Send flavour to processing channel
                self.proc_chn.0.send(flav).unwrap();
            }
            Err(_) => {
                let p = flav.dir().as_ref().unwrap().src_path.as_path();
                error!("Failed to prepare synchronization for {:?}", p);
            }
        }

        Ok(())
    }

    pub fn process(
        &self,
        flav: Box<dyn dir::Flavour + Send + Sync>,
        job: u8,
    ) -> Result<(), SyncError> {
        let p = flav.dir().as_ref().unwrap().src_path.as_path();
        let m = flav.method();
        trace!("Syncing {:?} with method {:?}", p, m);
        self.update_job(
            job,
            Some(stats::Info {
                category: flav.category(),
                name: flav.name().to_string(),
                desc: format!("{:?}", p),
            }),
        );
        match m {
            SyncMethod::Merge => flav.merge()?,
            SyncMethod::Duplicate => flav.dup()?,
        }

        Ok(())
    }

    /// If the scan session is complete.
    pub fn is_scanned(&self) -> bool {
        *self.scanned.lock().unwrap()
    }

    /// Helper for statistics update.
    pub fn todo_one(&self) {
        self.stats_inc(stats::Command::Todo);
    }

    /// Helper for statistics update.
    pub fn scanned_one(&self, job: u8) {
        self.update_job(job, None);
        self.stats_inc(stats::Command::Scanned);
    }

    /// Helper for statistics update.
    pub fn done_one(&self, job: u8) {
        self.update_job(job, None);
        self.stats_inc(stats::Command::Done);
    }

    /// Helper for statistics update.
    pub fn skip_one(&self) {
        self.stats_inc(stats::Command::Skipped);
    }

    /// Helper for statistics update.
    pub fn error_done(&self, job: u8) {
        self.update_job(job, None);
        self.stats_inc(stats::Command::Error);
    }

    /// Helper for statistics update.
    pub fn update_job(&self, job: u8, i: Option<stats::Info>) {
        self.stats_chn
            .send(stats::Transport {
                cmd: stats::Command::Job,
                val: job as i64,
                info: i,
            })
            .expect("Failed to send job update")
    }

    /// Helper function to send [stats::Command::Runtime] messages to
    /// [stats::Stats].
    pub fn send_log(&self, i: stats::Info) {
        self.stats_chn
            .send(stats::Transport {
                cmd: stats::Command::Log,
                val: 0,
                info: Some(i),
            })
            .expect("Failed to send log");
    }

    fn stats_inc(&self, c: stats::Command) {
        self.stats_chn
            .send(stats::Transport {
                cmd: c,
                val: 1,
                info: None,
            })
            .expect("Failed to increment stats counter");
    }
}
