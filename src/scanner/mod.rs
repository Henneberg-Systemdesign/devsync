// Copyright (C) 2022 Jochen Henneberg <jh@henneberg-systemdesign.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crossbeam::thread;
use log::{error, info, trace};

mod scan;
use super::dir;
use super::dir::Flavour;
use super::utils;
use super::Config;
use scan::Scan;
pub mod stats;

type WrappedScan = Arc<Scan>;

/// Scan job controller.
pub struct Scanner {
    /// Number of parallel scan jobs.
    jobs: u8,
    /// Scan object.
    scan: WrappedScan,
}

impl Scanner {
    /// Create new scanner and register all flavour templates.
    pub fn new(
        args: &getopts::Matches,
        src: &Path,
        target: &Path,
        stats: &stats::Stats,
        cfg: Arc<Config>,
    ) -> Self {
        Self {
            jobs: cfg.jobs,
            scan: Arc::new(
                Scan::new(src, target, stats, cfg)
                    .register(Box::new(dir::Yocto::template(args)))
                    .register(Box::new(dir::Cmake::template(args)))
                    .register(Box::new(dir::Flutter::template(args)))
                    .register(Box::new(dir::Meson::template(args)))
                    .register(Box::new(dir::Ninja::template(args)))
                    .register(Box::new(dir::Cargo::template(args)))
                    .register(Box::new(dir::Git::template(args)))
                    .register(Box::new(dir::Svn::template(args)))
                    .register(Box::new(dir::Simple::template(args))),
            ),
        }
    }

    /// Run the scans.
    pub fn run(&self) {
        info!(
            "Synchronize contents from {:?} with {:?}",
            self.scan.src_path, self.scan.target_path
        );
        // increment statistics
        self.scan.todo_one();
        thread::scope(|scope| {
            for j in 0..self.jobs {
                let scan = self.scan.clone();
                scope.spawn(move |_| loop {
                    match scan.scan_chn.1.recv_timeout(Duration::from_millis(100)) {
                        Ok((p, i)) => {
                            trace!("Scan path: {:?} on job {:?}", p, j);
                            match scan.scan(p.as_path(), i, j) {
                                Err(e) => {
                                    error!("Failed to scan {:?} because '{}'", p, e);
                                    scan.error_done(j);
                                }
                                Ok(_) => {
                                    trace!("Scan done path: {:?} on job {:?}", p, j);
                                    scan.done_one(j);
                                }
                            }
                        }
                        Err(_) => {
                            if scan.is_complete() {
                                break;
                            }
                        }
                    }
                });
            }

            // start scanning with the source directory
            self.scan
                .scan_chn
                .0
                .send((self.scan.src_path.clone(), None))
                .unwrap();
        })
        .expect("Failed to initialize thread pool");
    }
}
