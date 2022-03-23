// Copyright (C) 2022 Jochen Henneberg <jh@henneberg-systemdesign.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{Category, Dir, Flavour};

pub struct Cmake {
    dir: Box<Option<Dir>>,
    ignore: bool,
}

impl Flavour for Cmake {
    fn init_opts(opts: &mut getopts::Options) {
        opts.optflag("", "cmake-sync", "Sync CMake build directories");
    }

    fn template(args: &getopts::Matches) -> Self {
        Cmake {
            dir: Box::new(None),
            ignore: !args.opt_present("cmake-sync"),
        }
    }

    /// Look for file called 'CMakeCache.txt' to identify CMake build
    /// directory.
    fn probe(&self, d: &Dir) -> Option<Box<dyn Flavour>> {
        for d in &d.files {
            if d.file_name() == "CMakeCache.txt" {
                return Some(self.build());
            }
        }
        None
    }

    fn build(&self) -> Box<dyn Flavour> {
        Box::new(Cmake {
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
        Category::Build
    }

    /// Recurse if --cmake-sync is set.
    fn recurse(&self) -> bool {
        !self.skip()
    }

    /// Skip if --cmake-sync is not set.
    fn skip(&self) -> bool {
        self.ignore
    }

    fn name(&self) -> &'static str {
        "Cmake"
    }
}
