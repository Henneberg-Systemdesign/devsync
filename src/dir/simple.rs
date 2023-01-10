// Copyright (C) 2022 Jochen Henneberg <jh@henneberg-systemdesign.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{Dir, Flavour};

#[derive(Debug)]
pub struct Simple {
    dir: Box<Option<Dir>>,
}

impl Flavour for Simple {
    fn init_opts(_opts: &mut getopts::Options) {}

    fn template(_args: &getopts::Matches) -> Self {
        Simple {
            dir: Box::new(None),
        }
    }

    /// No probing, just return constructed flavour.
    fn probe(&self, _d: &Dir) -> Option<Box<dyn Flavour + Send + Sync>> {
        Some(self.build())
    }

    fn build(&self) -> Box<dyn Flavour + Send + Sync> {
        Box::new(Simple {
            dir: Box::new(None),
        })
    }

    fn stay(&self) -> bool {
        false
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
}
