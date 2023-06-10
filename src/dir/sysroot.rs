// Copyright (C) 2022 Jochen Henneberg <jh@henneberg-systemdesign.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

use bitflags::bitflags;

use super::{Category, Dir, Flavour};

pub struct Sysroot {
    dir: Box<Option<Dir>>,
    ignore: bool,
}

bitflags! {
    #[derive(PartialEq)]
    struct RequiredFiles: u8 {
        const NONE = 0;
        const BIN = 0x01;
        const ETC = 0x02;
        const LIB = 0x04;
        const USR = 0x08;
        const VAR = 0x10;
        const ALL = Self::BIN.bits() | Self::ETC.bits() | Self::LIB.bits() | Self::USR.bits() | Self::VAR.bits();
    }
}

impl Flavour for Sysroot {
    fn init_opts(opts: &mut getopts::Options) {
        opts.optflag("", "sysroot-sync", "Sync Sysroot directories");
    }

    fn template(args: &getopts::Matches) -> Self {
        Sysroot {
            dir: Box::new(None),
            ignore: !args.opt_present("sysroot-sync"),
        }
    }

    /// Look for directories 'dev', 'usr', 'var' and 'bin' to identify
    /// Sysroot directory.
    fn probe(&self, d: &Dir) -> Option<Box<dyn Flavour + Send + Sync>> {
        let mut m = RequiredFiles::NONE;
        for d in &d.dirs {
            let f = d.file_name().unwrap();
            if f == "bin" {
                m |= RequiredFiles::BIN;
            } else if f == "etc" {
                m |= RequiredFiles::ETC;
            } else if f == "lib" {
                m |= RequiredFiles::LIB;
            } else if f == "usr" {
                m |= RequiredFiles::USR;
            } else if f == "var" {
                m |= RequiredFiles::VAR;
            }

            if m == RequiredFiles::ALL {
                return Some(self.build());
            }
        }
        None
    }

    fn build(&self) -> Box<dyn Flavour + Send + Sync> {
        Box::new(Sysroot {
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
        Category::Special
    }

    /// Recurse if --sysroot-sync is set.
    fn recurse(&self) -> bool {
        !self.skip()
    }

    /// Skip if --sysroot-sync is not set.
    fn skip(&self) -> bool {
        self.ignore
    }

    fn name(&self) -> &'static str {
        "Sysroot"
    }
}
