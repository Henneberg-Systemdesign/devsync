// Copyright (C) 2022 Jochen Henneberg <jh@henneberg-systemdesign.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{Category, Dir, Flavour};

pub struct Svn {
    dir: Box<Option<Dir>>,
    ignore: bool,
}

impl Flavour for Svn {
    fn init_opts(opts: &mut getopts::Options) {
        opts.optflag("", "svn-ignore", "Ignore Svn repositories");
    }

    fn template(args: &getopts::Matches) -> Self {
        Svn {
            dir: Box::new(None),
            ignore: args.opt_present("svn-ignore"),
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
        })
    }

    fn set_dir(&mut self, d: Dir) {
        self.dir = Box::new(Some(d));
    }

    fn dir(&self) -> &Option<Dir> {
        &*self.dir
    }

    fn category(&self) -> Category {
        Category::Repository
    }

    /// Recurse if --svn-ignore is not set.
    fn recurse(&self) -> bool {
        !self.skip()
    }

    /// Recurse if --svn-ignore is set.
    fn skip(&self) -> bool {
        self.ignore
    }

    fn stay(&self) -> bool {
        false
    }

    fn name(&self) -> &'static str {
        "Subversion"
    }
}
