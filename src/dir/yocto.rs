// Copyright (C) 2022 Jochen Henneberg <jh@henneberg-systemdesign.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

use bitflags::bitflags;

use super::{Category, Dir, Flavour};

pub struct Yocto {
    dir: Box<Option<Dir>>,
    ignore: bool,
    ignore_downloads: bool,
    ignore_build: bool,
}

bitflags! {
    struct RequiredFiles: u8 {
        const NONE = 0;
        const BITBAKE = 1;
        const META = 2;
        const SCRIPTS = 4;
        const ALL = Self::BITBAKE.bits | Self::META.bits | Self::SCRIPTS.bits;
    }
}

impl Flavour for Yocto {
    fn init_opts(opts: &mut getopts::Options) {
        opts.optflag("", "yocto-ignore", "Ingore Yocto directories");
        opts.optflag("", "yocto-downloads-sync", "Sync downloads directory");
        opts.optflag("", "yocto-build-sync", "Sync build directory");
    }

    fn template(args: &getopts::Matches) -> Self {
        Yocto {
            dir: Box::new(None),
            ignore: args.opt_present("yocto-ignore"),
            ignore_downloads: !args.opt_present("yocto-downloads-sync"),
            ignore_build: !args.opt_present("yocto-build-sync"),
        }
    }

    fn probe(&self, d: &Dir) -> Option<Box<dyn Flavour>> {
        let mut m = RequiredFiles::NONE;
        for d in &d.dirs {
            if d.file_name() == "bitbake" {
                m |= RequiredFiles::BITBAKE;
            } else if d.file_name().to_str().unwrap().starts_with("meta") {
                m |= RequiredFiles::META;
            } else if d.file_name() == "scripts" {
                m |= RequiredFiles::SCRIPTS;
            }
            if m == RequiredFiles::ALL {
                return Some(self.build());
            }
        }
        None
    }

    fn build(&self) -> Box<dyn Flavour> {
        Box::new(Yocto {
            dir: Box::new(None),
            ignore: self.ignore,
            ignore_downloads: self.ignore_downloads,
            ignore_build: self.ignore_build,
        })
    }

    fn set_dir(&mut self, mut d: Dir) {
        // exclude downloads directory if exists
        if self.ignore_downloads {
            if let Some(i) = d.dirs.iter().position(|e| e.file_name() == "downloads") {
                d.dirs.swap_remove(i);
            }
        }

        // exclude build directory if exists
        if self.ignore_build {
            d.dirs.retain(|e| {
                e.file_name() != "build"
                    && e.file_name() != "BUILD"
                    && e.file_name() != "cache"
                    && e.file_name() != "sstate-cache"
                    && e.file_name() != "buildhistory"
            });
        }

        self.dir = Box::new(Some(d));
    }

    fn dir(&self) -> &Option<Dir> {
        &self.dir
    }

    fn category(&self) -> Category {
        Category::Special
    }

    fn recurse(&self) -> bool {
        !self.skip()
    }

    fn skip(&self) -> bool {
        self.ignore
    }

    fn name(&self) -> &'static str {
        "Yocto"
    }
}
