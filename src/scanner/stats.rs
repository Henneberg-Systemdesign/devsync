// Copyright (C) 2022 Jochen Henneberg <jh@henneberg-systemdesign.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::{Arc, Mutex};

use crossbeam::channel::{unbounded, Receiver, Sender};
use log::trace;

use super::dir;

/// Command type of channel transport.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Command {
    /// Modify [Stats::todo] counter.
    Todo,
    /// Modify [Stats::scanned] counter.
    Scanned,
    /// Modify [Stats::done] counter.
    Done,
    /// Modify [Stats::skipped] counter.
    Skipped,
    /// Modify [Stats::error] counter.
    Error,
    /// Signals non-fatal runtime error.
    Runtime,
    /// Signals entries for the log file.
    Log,
    /// Signals that scanning is complete.
    ScanComplete,
    /// Signals that processing is complete.
    Complete,
    /// Signals job details for job id.
    Job,
}

/// Detailed command info, used for [Command::Runtime], [Command::Log]
/// and [Command::Job] transports.
#[derive(Debug, Clone)]
pub struct Info {
    /// Flavour category.
    pub category: dir::Category,
    /// Flavour name.
    pub name: String,
    /// Description of incidence.
    pub desc: String,
}

/// Channel transport for statistics.
pub struct Transport {
    /// The command.
    pub cmd: Command,
    /// Value, most often used for counts.
    pub val: i64,
    /// Optional extra data.
    pub info: Option<Info>,
}

/// Statistics.
#[derive(Debug)]
pub struct Stats {
    /// Directories that need to be scanned.
    pub todo: i64,
    /// Directories that need to be processed.
    pub scanned: i64,
    /// Directories that have been processed.
    pub done: i64,
    /// Directories that have been skipped.
    pub skipped: i64,
    /// Directories that have not been processed due to errors.
    pub error: i64,
    /// Channels for transport, single reader multiple writers.
    pub chn: (Sender<Transport>, Receiver<Transport>),
    /// Set if scan is complete.
    pub scan_done: Arc<Mutex<bool>>,
    /// Set if processing is complete.
    pub proc_done: Arc<Mutex<bool>>,
}

impl Default for Stats {
    fn default() -> Self {
        Stats {
            todo: 0,
            scanned: 0,
            done: 0,
            skipped: 0,
            error: 0,
            chn: unbounded::<Transport>(),
            scan_done: Arc::new(Mutex::new(false)),
            proc_done: Arc::new(Mutex::new(false)),
        }
    }
}

impl Stats {
    /// Parse and eval new transport on [Self].
    pub fn process(&mut self, t: &Transport) -> Command {
        match t.cmd {
            Command::Todo => self.todo += t.val,
            Command::Scanned => self.scanned += t.val,
            Command::Done => self.done += t.val,
            Command::Skipped => self.skipped += t.val,
            Command::Error => self.error += t.val,
            _ => (),
        }

        if self.scan_complete() {
            let mut c = self.scan_done.lock().unwrap();
            if !*c {
                *c = true;
                trace!("Signal backup scanned");
                self.chn
                    .0
                    .send(Transport {
                        cmd: Command::ScanComplete,
                        val: 0,
                        info: None,
                    })
                    .unwrap();
            }
        }

        if self.proc_complete() {
            let mut c = self.proc_done.lock().unwrap();
            *c = true;
            trace!("Signal backup complete");
            self.chn
                .0
                .send(Transport {
                    cmd: Command::Complete,
                    val: 0,
                    info: None,
                })
                .unwrap();
        }

        t.cmd
    }

    /// Get sender channel reference, e. g. for cloning.
    pub fn sender(&self) -> &Sender<Transport> {
        &self.chn.0
    }

    fn scan_complete(&self) -> bool {
        self.todo == self.scanned + self.error
    }

    fn proc_complete(&self) -> bool {
        self.todo == self.done + self.error
    }
}
