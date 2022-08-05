// Copyright (C) 2022 Jochen Henneberg <jh@henneberg-systemdesign.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::process::{Command, Stdio};

use xml::EventReader;
use xml::name::OwnedName;
use xml::reader::XmlEvent;
use log::{trace};

use super::utils::SyncError;
use super::{Category, Dir, Flavour};

pub struct Svn {
    dir: Box<Option<Dir>>,
    ignore: bool,
    full: bool,
    ignore_unversioned: bool,
    ignore_modified: bool,
}

impl Svn {
    fn dup_all(&self) -> Result<(), SyncError> {
        trace!("Run svn command to get status");
        let svn = Command::new("svn")
            .arg("status")
            .arg("--xml")
            .stdout(Stdio::piped())
            .spawn()?;

        trace!("Parse XML output");
        let mut reader = EventReader::new(svn.stdout.unwrap());
        let t = std::thread::spawn(move || loop {
            if let Ok(e) = reader.next() {
                match e {
                    XmlEvent::StartElement{name: OwnedName{local_name: n, ..}, ..} => trace!("Start element found: {}", n),
                    XmlEvent::EndElement{name: OwnedName{local_name: n, ..}} => trace!("Start element found: {}", n),
                    XmlEvent::EndDocument => break,
                    _ => ()
                }
            }
        });
        trace!("XML parsing done");

        let _ = t.join();

        Ok(())
    }
}

impl Flavour for Svn {
    fn init_opts(opts: &mut getopts::Options) {
        opts.optflag("", "svn-ignore", "Ignore Svn repositories");
        opts.optflag("", "svn-full", "Full backup (default is unversioned and modified)");
        opts.optflag("", "svn-ignore-unversioned", "Don't backup unversioned files");
        opts.optflag("", "svn-ignore-modified", "Don't backup modified files");
    }

    fn template(args: &getopts::Matches) -> Self {
        Svn {
            dir: Box::new(None),
            ignore: args.opt_present("svn-ignore"),
            full: args.opt_present("svn-full"),
            ignore_unversioned: args.opt_present("svn-ignore-unversioned"),
            ignore_modified: args.opt_present("svn-ignore-modified"),
        }
    }

    /// Look for file '.svn' to identify Subversion directory.
    fn probe(&self, d: &Dir) -> Option<Box<dyn Flavour>> {
        for d in &d.dirs {
            if d.file_name() == ".svn" {
                return Some(self.build());
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
        self.full
    }

    /// Recurse if --svn-ignore is set.
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
