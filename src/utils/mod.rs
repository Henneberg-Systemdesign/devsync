// Copyright (C) 2022 Jochen Henneberg <jh@henneberg-systemdesign.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs;
use std::fs::DirEntry;
use std::option::Option;
use std::path::Path;

use cfg_match::cfg_match;
use log::trace;

use super::{ARGS_FILE, LOG_FILE};

#[derive(Debug)]
pub enum SyncError {
    // OptArg(String, String),
    Failed(String),
    Io(std::io::Error),
}

impl std::error::Error for SyncError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SyncError::Io(ref io) => Some(io),
            _ => None,
        }
    }
}

impl From<std::io::Error> for SyncError {
    fn from(err: std::io::Error) -> Self {
        SyncError::Io(err)
    }
}

impl From<git2::Error> for SyncError {
    fn from(err: git2::Error) -> Self {
        SyncError::Failed(format!("git: {}", err))
    }
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // SyncError::OptArg(opt, arg) => {
            //     write!(fmt, "Invalid option argument {} for {}", arg, opt)
            // }
            SyncError::Failed(reason) => write!(fmt, "Operation failed because: {}", reason),
            SyncError::Io(ref io) => std::fmt::Display::fmt(io, fmt),
        }
    }
}

/// Create directory but fist remove all entries recursively.
pub fn create_dir_save(p: &Path, delete: bool) -> Result<(), SyncError> {
    if p.exists() && delete {
        let _ = fs::remove_dir_all(p);
    }

    if !p.exists() {
        fs::create_dir_all(p)?;
    }

    Ok(())
}

/// Get all directories and files from path. The entries can be
/// filtered.
pub fn save_dirs_and_files(
    p: &Path,
    dirs: &mut Vec<DirEntry>,
    files: &mut Vec<DirEntry>,
    filter: Option<&[String]>,
    owned: bool,
) -> Result<(), SyncError> {
    for e in fs::read_dir(p)? {
        match e {
            Ok(e) => {
                // is this ours
                if owned {
                    if let Ok(false) = test_file_owned(e.path().as_path()) {
                        trace!("File {:?} not owned by us", e);
                        continue;
                    }
                }

                // first check if we should ignore this
                if let Some(f) = filter {
                    if f.iter()
                        .any(|p| e.path().as_path().to_str().unwrap().ends_with(p))
                    {
                        trace!("File {:?} filtered", e);
                        continue;
                    }
                }

                let t = e.file_type().unwrap();
                if t.is_file() && e.file_name() != ARGS_FILE && e.file_name() != LOG_FILE {
                    files.push(e);
                } else if t.is_dir() && e.path() != p {
                    dirs.push(e);
                }
            }
            Err(_) => continue,
        }
    }

    Ok(())
}

/// Remove all directories (recursively) and files from path.
pub fn rm_dirs_and_files(p: &Path) -> Result<(), SyncError> {
    for e in fs::read_dir(p)? {
        match e {
            Ok(e) => {
                let t = e.file_type().unwrap();
                if t.is_file() && e.file_name() != ARGS_FILE && e.file_name() != LOG_FILE {
                    fs::remove_file(e.path().as_path())?;
                } else if t.is_dir() && e.path() != p {
                    fs::remove_dir_all(e.path().as_path())?;
                }
            }
            Err(_) => continue,
        }
    }

    Ok(())
}

/// Apply filter to directory entries vector.
pub fn filter_dir_entries(a: &Vec<DirEntry>, b: &mut Vec<DirEntry>) {
    for e in a {
        if let Some(i) = b.iter().position(|p| p.file_name() == e.file_name()) {
            b.remove(i);
        }
    }
}

/// Copy file with relative path.
pub fn cp_r(s: &Path, t: &Path, f: &Path, archive: bool) -> Result<(), SyncError> {
    let sf = s.join(f);
    let tf = t.join(f);

    trace!("Copying {:?} to {:?}", sf, tf);
    match fs::copy(&sf, &tf) {
        Err(_) => Err(SyncError::Failed(format!(
            "Failed to copy {:?} to {:?}",
            sf, tf
        )))?,
        Ok(_) => {
            if archive {
                set_file_timestamps(&sf, &tf)?;
                set_file_permissions(&sf, &tf)?;
            }
        }
    }

    Ok(())
}

/// Copy file with relative path and create directory if needed.
pub fn cp_r_d(s: &Path, t: &Path, f: &Path, archive: bool) -> Result<(), SyncError> {
    if let Some(p) = f.parent() {
        fs::create_dir_all(&t.join(p))?;
    }
    cp_r(s, t, f, archive)
}

/// Copy file with absolute path.
pub fn cp(s: &Path, t: &Path, f: &Path, archive: bool) -> Result<(), SyncError> {
    let p = f.strip_prefix(s).unwrap();
    cp_r(s, t, p, archive)
}

/// Check if a file has changed by comparing the last-modified timestamps.
pub fn diff(s: &Path, t: &Path, f: &DirEntry) -> bool {
    let fp = f.path();
    let p = fp.strip_prefix(s).unwrap();
    let t = p.join(t).join(f.file_name());

    trace!("Check diff of {:?} vs {:?}", s, t);
    match fs::metadata(t) {
        Ok(m) => {
            m.modified().unwrap() != f.metadata().unwrap().modified().unwrap()
                || m.permissions() != f.metadata().unwrap().permissions()
        }
        Err(_) => true,
    }
}

/// Set file timestamps.
pub fn set_file_timestamps(s: &Path, t: &Path) -> Result<(), SyncError> {
    let ok = cfg_match! {
        unix => set_file_timestamps_unix(s, t),
        _ => unimplemented!("Support for this OS is imcomplete"),
    };

    if ok {
        Ok(())
    } else {
        Err(SyncError::Failed(format!(
            "Failed to copy atime and mtime from {:?} to {:?}",
            &s, &t
        )))
    }
}

/// Set file permissions.
pub fn set_file_permissions(s: &Path, t: &Path) -> Result<(), SyncError> {
    let p = fs::metadata(s)?.permissions();
    match fs::set_permissions(t, p) {
        Ok(_) => Ok(()),
        Err(e) => Err(SyncError::Io(e)),
    }
}

/// Test if file or directory are owned by this user.
pub fn test_file_owned(f: &Path) -> Result<bool, SyncError> {
    cfg_match! {
        unix => test_file_owned_unix(f),
        _ => unimplemented!("Support for this OS is imcomplete"),
    }
}

#[cfg(all(unix))]
use std::os::unix::fs::MetadataExt;
fn test_file_owned_unix(f: &Path) -> Result<bool, SyncError> {
    let file_uid = fs::metadata(f)?.uid();
    let my_uid = users::get_current_uid();
    Ok(file_uid == my_uid)
}

use std::os::unix::io::AsRawFd;
use std::time;
fn set_file_timestamps_unix(s: &Path, t: &Path) -> bool {
    if let Ok(m) = fs::metadata(s) {
        if let Ok(f) = fs::File::open(t) {
            let mut t = m
                .accessed()
                .unwrap()
                .duration_since(time::SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as i64;
            let atime = libc::timespec {
                tv_sec: t / 1_000_000_000,
                tv_nsec: t % 1_000_000_000,
            };
            t = m
                .modified()
                .unwrap()
                .duration_since(time::SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as i64;
            let mtime = libc::timespec {
                tv_sec: t / 1_000_000_000,
                tv_nsec: t % 1_000_000_000,
            };
            unsafe {
                return libc::futimens(f.as_raw_fd(), [atime, mtime].as_ptr()) == 0;
            }
        }
    }
    false
}
