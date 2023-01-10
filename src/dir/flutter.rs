// Copyright (C) 2022 Jochen Henneberg <jh@henneberg-systemdesign.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{Category, Dir, Flavour};

pub struct Flutter {
    dir: Box<Option<Dir>>,
    ignore: bool,
}

impl Flavour for Flutter {
    fn init_opts(opts: &mut getopts::Options) {
        opts.optflag("", "flutter-sync", "Sync Flutter build directories");
    }

    fn template(args: &getopts::Matches) -> Self {
        Flutter {
            dir: Box::new(None),
            ignore: !args.opt_present("flutter-sync"),
        }
    }

    /// Look for file ending on .cache.dill.track.dill to identify
    /// Flutter build directory.
    fn probe(&self, d: &Dir) -> Option<Box<dyn Flavour + Send + Sync>> {
        for f in &d.files {
            if f.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .ends_with(".cache.dill.track.dill")
            {
                return Some(self.build());
            }
        }
        None
    }

    fn build(&self) -> Box<dyn Flavour + Send + Sync> {
        Box::new(Flutter {
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

    /// Recurse if --flutter-sync is set.
    fn recurse(&self) -> bool {
        !self.skip()
    }

    /// Skip if --flutter-sync is set.
    fn skip(&self) -> bool {
        self.ignore
    }

    fn name(&self) -> &'static str {
        "Flutter"
    }
}
