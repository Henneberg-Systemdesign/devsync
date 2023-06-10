// Copyright (C) 2022 Jochen Henneberg <jh@henneberg-systemdesign.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

use bitflags::bitflags;

use super::{Category, Dir, Flavour};

pub struct Meson {
    dir: Box<Option<Dir>>,
    ignore: bool,
}

bitflags! {
    #[derive(PartialEq)]
    struct RequiredFiles: u8 {
        const NONE = 0;
        const INFO = 1;
        const LOGS = 2;
        const PRIV = 4;
        const ALL = Self::INFO.bits() | Self::LOGS.bits() | Self::PRIV.bits();
    }
}

impl Flavour for Meson {
    fn init_opts(opts: &mut getopts::Options) {
        opts.optflag("", "meson-sync", "Sync Meson build directories");
    }

    fn template(args: &getopts::Matches) -> Self {
        Meson {
            dir: Box::new(None),
            ignore: !args.opt_present("meson-sync"),
        }
    }

    /// Look for directories 'meson-info', 'meson-logs' and
    /// 'meson-private' to identify Meson build directory.
    fn probe(&self, d: &Dir) -> Option<Box<dyn Flavour + Send + Sync>> {
        let mut m = RequiredFiles::NONE;
        for d in &d.files {
            let f = d.file_name().unwrap();
            if f == "meson-info" {
                m |= RequiredFiles::INFO;
            } else if f == "meson-logs" {
                m |= RequiredFiles::LOGS;
            } else if f == "meson-private" {
                m |= RequiredFiles::PRIV;
            }
            if m == RequiredFiles::ALL {
                return Some(self.build());
            }
        }
        None
    }

    fn build(&self) -> Box<dyn Flavour + Send + Sync> {
        Box::new(Meson {
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

    /// Recurse if --meson-sync is set.
    fn recurse(&self) -> bool {
        !self.skip()
    }

    /// Skip if --meson-sync is not set.
    fn skip(&self) -> bool {
        self.ignore
    }

    fn name(&self) -> &'static str {
        "Meson"
    }
}
