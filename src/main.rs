// Copyright (C) 2022 Jochen Henneberg <jh@henneberg-systemdesign.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

extern crate getopts;

use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::vec::Vec;

use log::{error, info, trace, warn};
use simple_logger::SimpleLogger;

mod dir;
use crate::dir::Flavour;
mod scanner;
use scanner::{stats, Scanner};
mod ui;
mod utils;

const DEFAULT_JOBS: u8 = 10;
const ARGS_FILE: &str = ".devsync";

/// Global configuration date.
#[derive(Debug, Clone)]
pub struct Config {
    /// Number of parallel jobs.
    jobs: u8,
    /// If extraneous files and directories shall be deleted.
    delete: bool,
    /// If copying shall happen in archive mode (preserving
    /// timestamps, ownership and permissions)
    archive: bool,
    /// Files and directories to be ignored.
    ignore: Vec<String>,
}

/// Prints help page.
fn usage(program: &str, opts: getopts::Options, err: Option<getopts::Fail>) {
    if err.is_some() {
        let msg = format!("Error: {}", err.unwrap());
        println!("{}\n", msg)
    }
    let brief = format!("Usage: {} [options]", program);
    print!("{}", opts.usage(&brief));
}

/// Read command line arguments from file.
fn read_args_from_file() -> Result<String, std::io::Error> {
    let mut a: String = String::new();
    let mut f = std::fs::File::open(ARGS_FILE)?;
    info!("Read arguments from session file");
    f.read_to_string(&mut a)?;
    Ok(a)
}

/// Write command line arguments to file.
fn write_args_to_file(args: &[String], p: &Path) -> Result<(), std::io::Error> {
    let sf = p.join(ARGS_FILE);
    let mut f = std::fs::File::create(&sf)?;
    info!("Write '{}' to session file {:?}", args[1..].join(" "), sf);
    let _ = f.write(args[1..].join("\0").as_bytes())?;
    Ok(())
}

/// Entry point.
fn main() {
    let mut raw_args: Vec<String> = std::env::args().collect();
    let program = raw_args[0].clone();
    let mut opts = getopts::Options::new();
    let mut session_file = false;

    SimpleLogger::new()
        .with_level(log::LevelFilter::Off)
        .env()
        .with_colors(true)
        .init()
        .unwrap();

    opts.optflag("h", "help", "Usage hints");
    opts.optopt("s", "source", "Source directory", "DIR");
    opts.optopt("t", "target", "Target directory", "DIR");
    opts.optflag("d", "delete", "Remove extraneous files");
    opts.optflag("a", "archive", "Preserve timestamps");
    opts.optflag("u", "ui", "Show terminal user interface");
    opts.optflag("i", "ignore", "List of directory or file names to ignore");
    opts.optopt("j", "jobs", "Parallel jobs (1 - 255, default is 10)", "NUM");

    // we have to get the flavour specific options
    dir::Yocto::init_opts(&mut opts);
    dir::Cmake::init_opts(&mut opts);
    dir::Flutter::init_opts(&mut opts);
    dir::Meson::init_opts(&mut opts);
    dir::Ninja::init_opts(&mut opts);
    dir::Cargo::init_opts(&mut opts);
    dir::Git::init_opts(&mut opts);
    dir::Svn::init_opts(&mut opts);
    dir::Simple::init_opts(&mut opts);

    // if we do not have sufficient arguments try to get them from a
    // previous session file
    if raw_args.len() == 1 {
        match read_args_from_file() {
            Ok(a) => {
                raw_args.append(&mut a.split('\0').map(String::from).collect());
                session_file = true;
                trace!("Using command line arguments from session file");
            }
            Err(_) => {
                usage(&program, opts, None);
                return;
            }
        }
    }

    // parse and handle command line args
    let args = match opts.parse(&raw_args[1..]) {
        Ok(m) => m,
        Err(e) => {
            usage(&program, opts, Some(e));
            return;
        }
    };

    if args.opt_present("help") {
        usage(&program, opts, None);
        return;
    }

    // these are required, but optional because of 'h'
    if !args.opt_present("s") || !args.opt_present("t") {
        error!("Missing source or target path");
        usage(&program, opts, None);
        return;
    }

    // prepare our scanner
    let src = match Path::new(&args.opt_str("s").unwrap()).canonicalize() {
        Ok(p) => p,
        Err(_) => panic!("Invalid source path"),
    };

    let t = &args.opt_str("t").unwrap();
    let target = match Path::new(t).canonicalize() {
        Ok(p) => p,
        Err(_) => {
            fs::create_dir_all(t).expect("Cannot create target path");
            Path::new(t).canonicalize().unwrap()
        }
    };

    // write session file
    if !session_file {
        if let Some(i) = raw_args.iter().position(|p| p == "-s" || p == "--source") {
            raw_args[i + 1] = src.to_str().unwrap().to_string();
        }
        if let Some(i) = raw_args.iter().position(|p| p == "-t" || p == "--target") {
            raw_args[i + 1] = target.to_str().unwrap().to_string();
        }
        write_args_to_file(&raw_args, &target).expect("Cannot write session file");
    }

    let cfg = Arc::new(Config {
        jobs: args.opt_get_default("jobs", DEFAULT_JOBS).unwrap(),
        delete: args.opt_present("delete"),
        archive: args.opt_present("archive"),
        ignore: match args.opt_str("ignore") {
            Some(a) => a.split(',').map(String::from).collect(),
            _ => vec![],
        },
    });

    let mut stats = stats::Stats::default();
    let scanner = Scanner::new(&args, &src, &target, &stats, cfg.clone());

    let stats_th = if args.opt_present("u") {
        let mut ui = ui::TermUi::new(stats, cfg).unwrap();
        thread::spawn(move || {
            ui.run().expect("Failed to run ui");
        })
    } else {
        // track statistics updates
        thread::spawn(move || loop {
            if let Ok(t) = stats.chn.1.recv() {
                match stats.process(&t) {
                    stats::Command::Complete => break,
                    stats::Command::Job => {
                        info!("Stats: Job {:?} on {:?}", t.val, &t.info)
                    }
                    stats::Command::Runtime => {
                        let i = t.info.unwrap();
                        warn!(
                            "Runtime from flavour {}({}): {}",
                            i.name, i.category, i.desc
                        )
                    }
                    _ => info!("Stats: {:?}", stats),
                }
            }
        })
    };

    // start syncing
    scanner.run();
    stats_th.join().unwrap();
}
