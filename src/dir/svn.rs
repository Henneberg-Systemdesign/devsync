// Copyright (C) 2022 Jochen Henneberg <jh@henneberg-systemdesign.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crossbeam::thread;
use log::trace;
use xml::name::OwnedName;
use xml::reader::XmlEvent;
use xml::EventReader;

use super::utils::SyncError;
use super::{stats, utils, Category, Dir, Flavour};

pub struct Svn {
    dir: Box<Option<Dir>>,
    ignore: bool,
    full: bool,
    ignore_unversioned: bool,
    ignore_modified: bool,
}

enum Reason {
    None,
    Modified,
    Unversioned,
}

impl From<String> for Reason {
    fn from(s: String) -> Self {
        trace!("Map string {} to reason", s);
        match s.as_str() {
            "modified" => Reason::Modified,
            "unversioned" => Reason::Unversioned,
            _ => Reason::None,
        }
    }
}

impl Svn {
    fn cp(s: &Path, t: &Path, f: &Path) -> Result<(), SyncError> {
        utils::cp_d(s, t, f, false)?;
        Ok(())
    }

    fn dir_unchecked(&self) -> &Dir {
        match self.dir.as_ref() {
            Some(d) => d,
            None => panic!("Flavours 'dir' entry is None"),
        }
    }

    fn dup_all(&self) -> Result<(), SyncError> {
        if let Some(d) = self.dir() {
            utils::rm_dirs_and_files(d.target_path.as_path())?;

            let svn = Command::new("svn")
                .arg("status")
                .arg("--xml")
                .arg(self.dir_unchecked().src_path.as_path())
                .stdout(Stdio::piped())
                .spawn()?;

            // our xml parser
            let mut reader = EventReader::new(svn.stdout.unwrap());
            // remember the file to handle, the file path as well as
            // modified|unversioned
            let mut file: (Option<PathBuf>, Option<Reason>) = (None, None);

            // reader thread for svn command output
            thread::scope(|scope| {
                scope.spawn(|_| loop {
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
                                        trace!(
                                            "Handle wc-status {} {}",
                                            &a.name.local_name,
                                            &a.value
                                        );
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
                                    if let Err(e) =
                                        Self::cp(&d.src_path, &d.target_path.join("modified"), f)
                                    {
                                        d.send_runtime(stats::Info {
                                            category: self.category(),
                                            name: String::from(self.name()),
                                            desc: format!(
                                                "Failed to backup modified file {:?} because {}",
                                                f, e
                                            ),
                                        });
                                    }
                                }
                                file = (None, None);
                            }
                            (Some(f), Some(Reason::Unversioned)) => {
                                if !self.ignore_unversioned {
                                    if let Err(e) =
                                        Self::cp(&d.src_path, &d.target_path.join("unversioned"), f)
                                    {
                                        d.send_runtime(stats::Info {
                                            category: self.category(),
                                            name: String::from(self.name()),
                                            desc: format!(
                                                "Failed to backup unversioned file {:?} because {}",
                                                f, e
                                            ),
                                        });
                                    }
                                }
                                file = (None, None);
                            }
                            (_, Some(Reason::None)) => file = (None, None),
                             _ => (),
                        }
                    }
                });
            })
            .unwrap();

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
