// Copyright (C) 2022 Jochen Henneberg <jh@henneberg-systemdesign.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{Category, Dir, Flavour};

pub struct Cargo {
    dir: Box<Option<Dir>>,
    ignore: bool,
}

impl Flavour for Cargo {
    fn init_opts(opts: &mut getopts::Options) {
        opts.optflag("", "cargo-sync", "Sync Cargo build directories");
    }

    fn template(args: &getopts::Matches) -> Self {
        Cargo {
            dir: Box::new(None),
            ignore: !args.opt_present("cargo-sync"),
        }
    }

    /// Look for file called 'CACHEDIR.TAG' to identify Cargo build
    /// directory.
    fn probe(&self, d: &Dir) -> Option<Box<dyn Flavour + Send + Sync>> {
        for d in &d.files {
            if d.file_name().unwrap() == "CACHEDIR.TAG" {
                return Some(self.build());
            }
        }
        None
    }

    fn build(&self) -> Box<dyn Flavour + Send + Sync> {
        Box::new(Cargo {
            dir: Box::new(None),
            ignore: self.ignore,
        })
    }

    fn set_dir(&mut self, d: Dir) {
        self.dir = Box::new(Some(d));
    }

    fn dir(&self) -> &Option<Dir> {
        &self.dir
    }

    fn dir_mut(&mut self) -> &mut Option<Dir> {
        &mut self.dir
    }

    fn category(&self) -> Category {
        Category::Build
    }

    /// Recurse if --cargo-sync is set.
    fn recurse(&self) -> bool {
        !self.skip()
    }

    /// Skip if --cargo-sync is not set.
    fn skip(&self) -> bool {
        self.ignore
    }

    fn name(&self) -> &'static str {
        "Cargo"
    }
}
