// Copyright (C) 2022 Jochen Henneberg <jh@henneberg-systemdesign.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs;
use std::io::Write;
use std::option::Option;
use std::path::{Path, PathBuf};

use cfg_match::cfg_match;
use log::trace;

use super::scanner::stats;
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

pub fn log_stats_info(log_file: &mut fs::File, prefix: &str, i: &stats::Info) {
    writeln!(
        log_file,
        "{} {}({}): {}",
        prefix, i.name, i.category, i.desc
    )
    .expect("Cannot write to log file");
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
    dirs: &mut Vec<PathBuf>,
    files: &mut Vec<PathBuf>,
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
                    files.push(e.path());
                } else if t.is_dir() && e.path() != p {
                    dirs.push(e.path());
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
pub fn filter_dir_entries(a: &Vec<PathBuf>, b: &mut Vec<PathBuf>) {
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
        fs::create_dir_all(t.join(p))?;
    }
    cp_r(s, t, f, archive)
}

/// Copy file with absolute path.
pub fn cp(s: &Path, t: &Path, f: &Path, archive: bool) -> Result<(), SyncError> {
    let p = f.strip_prefix(s).unwrap();
    cp_r(s, t, p, archive)
}

/// Copy file with absolute path and create directory if needed.
pub fn cp_d(s: &Path, t: &Path, f: &Path, archive: bool) -> Result<(), SyncError> {
    let p = f.strip_prefix(s).unwrap();
    cp_r_d(s, t, p, archive)
}

/// Check if a file has changed by comparing the last-modified timestamps.
pub fn diff(s: &Path, t: &Path, f: &Path) -> bool {
    let p = f.strip_prefix(s).unwrap();
    let t = p.join(t).join(f.file_name().unwrap());

    trace!("Check diff of {:?} vs {:?}", s, t);
    match fs::metadata(t) {
        Ok(m) => {
            m.modified().unwrap() < f.metadata().unwrap().modified().unwrap()
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

#[cfg(test)]
mod test {
    use super::*;
    use std::path::PathBuf;

    fn path() -> PathBuf {
        let mut r = PathBuf::new();
        r.push(env!("CARGO_MANIFEST_DIR"));
        r.push("tests");
        r
    }

    fn sample_dir(p: &Path) {
        create_dir_save(p, true).expect("Failed to create path");
        create_dir_save(&p.join("dir_d"), false).expect("Failed to create path");
        create_dir_save(&p.join("dir_f"), false).expect("Failed to create path");
        let _ = fs::File::create(p.join("file_a"));
        let _ = fs::File::create(p.join("file_b"));
        let _ = fs::File::create(p.join("file_c"));
        let _ = fs::File::create(p.join("file_e"));
        let _ = fs::File::create(p.join("dir_d").join("file_a"));
        let _ = fs::File::create(p.join("dir_d").join("file_b"));
    }

    #[test]
    fn test_create_dir_save() {
        let mut p = path();
        p.push("create_dir_save");
        create_dir_save(&p, false).expect("Failed to create path");
        assert!(p.exists());
        let _ = fs::File::create(p.join("some_file"));
        create_dir_save(&p, true).expect("Failed to create path");
        assert!(!p.join("some_file").exists());

        // cleanup
        let _ = fs::remove_dir_all(p);
    }

    #[test]
    fn test_save_dirs_and_files() {
        let mut p = path();
        p.push("save_dirs_and_files");
        sample_dir(&p);
        let mut f: Vec<PathBuf> = Vec::new();
        let mut d: Vec<PathBuf> = Vec::new();
        let _ = save_dirs_and_files(&p, &mut d, &mut f, None, false);
        assert!(f.len() == 4);
        assert!(d.len() == 2);
        f.clear();
        d.clear();
        let _ = save_dirs_and_files(&p, &mut d, &mut f, None, true);
        assert!(f.len() == 4);
        assert!(d.len() == 2);
        f.clear();
        d.clear();
        save_dirs_and_files(
            &p,
            &mut d,
            &mut f,
            Some(&["file_b".to_string(), "d".to_string()]),
            true,
        )
        .expect("Failed to scan path");
        assert!(f.len() == 3);
        assert!(d.len() == 1);

        // cleanup
        let _ = fs::remove_dir_all(p);
    }

    #[test]
    fn test_rm_dirs_and_files() {
        let mut p = path();
        p.push("rm_dirs_and_files");
        sample_dir(&p);
        assert!(p.join("file_a").exists());
        let _ = rm_dirs_and_files(&p);
        assert!(p.exists());
        assert!(!p.join("file_a").exists());
        assert!(!p.join("dir_d").exists());

        // cleanup
        let _ = fs::remove_dir_all(p);
    }

    #[test]
    fn test_filter_dir_entries() {
        let p = path();
        sample_dir(&p.join("filter_dir_entries_1"));
        sample_dir(&p.join("filter_dir_entries_2"));
        let mut f1: Vec<PathBuf> = Vec::new();
        let mut d1: Vec<PathBuf> = Vec::new();
        let mut f2: Vec<PathBuf> = Vec::new();
        let mut d2: Vec<PathBuf> = Vec::new();
        let _ = save_dirs_and_files(
            &p.join("filter_dir_entries_1"),
            &mut d1,
            &mut f1,
            None,
            true,
        );
        let _ = save_dirs_and_files(
            &p.join("filter_dir_entries_2"),
            &mut d2,
            &mut f2,
            None,
            true,
        );
        filter_dir_entries(&f1, &mut f2);
        filter_dir_entries(&d1, &mut d2);
        assert!(f2.is_empty());
        assert!(d2.is_empty());

        // cleanup
        let _ = fs::remove_dir_all(p.join("filter_dir_entries_1"));
        let _ = fs::remove_dir_all(p.join("filter_dir_entries_2"));
    }

    #[test]
    fn test_cp_r_and_diff() {
        let p = path();
        sample_dir(&p.join("cp_r_1"));
        // ensure timestamps differs
        std::thread::sleep(std::time::Duration::new(1, 0));

        let _ = create_dir_save(&p.join("cp_r_2"), true);
        let _ = cp_r(
            &p.join("cp_r_1"),
            &p.join("cp_r_2"),
            Path::new("file_a"),
            false,
        );
        assert!(p.join("cp_r_2").join("file_a").exists());
        for f in fs::read_dir(p.join("cp_r_2")).unwrap().flatten() {
            let t = f.file_type().unwrap();
            if t.is_file() {
                assert!(diff(&p.join("cp_r_2"), &p.join("cp_r_1"), &f.path()));
            }
        }

        let _ = cp_r(
            &p.join("cp_r_1"),
            &p.join("cp_r_2"),
            Path::new("file_a"),
            true,
        );
        assert!(p.join("cp_r_2").join("file_a").exists());
        for f in fs::read_dir(p.join("cp_r_2")).unwrap().flatten() {
            let t = f.file_type().unwrap();
            if t.is_file() {
                assert!(!diff(&p.join("cp_r_2"), &p.join("cp_r_1"), &f.path()));
            }
        }

        // cleanup
        let _ = fs::remove_dir_all(p.join("cp_r_1"));
        let _ = fs::remove_dir_all(p.join("cp_r_2"));
    }

    #[test]
    fn test_cp_r_d() {
        let p = path();
        sample_dir(&p.join("cp_r_d_1"));

        let _ = cp_r_d(
            &p.join("cp_r_d_1"),
            &p.join("cp_r_d_2"),
            Path::new("file_a"),
            false,
        );
        assert!(p.join("cp_r_d_2").join("file_a").exists());

        // cleanup
        let _ = fs::remove_dir_all(p.join("cp_r_d_1"));
        let _ = fs::remove_dir_all(p.join("cp_r_d_2"));
    }

    #[test]
    fn test_cp() {
        let p = path();
        sample_dir(&p.join("cp_1"));
        sample_dir(&p.join("cp_2"));

        let _ = cp(
            &p.join("cp_1"),
            &p.join("cp_2"),
            &p.join("cp_1").join("file_a"),
            false,
        );
        assert!(p.join("cp_2").join("file_a").exists());

        // cleanup
        let _ = fs::remove_dir_all(p.join("cp_1"));
        let _ = fs::remove_dir_all(p.join("cp_2"));
    }

    #[test]
    fn test_cp_d() {
        let p = path();
        sample_dir(&p.join("cp_d_1"));

        let _ = cp_d(
            &p.join("cp_d_1"),
            &p.join("cp_d_2"),
            &p.join("cp_d_1").join("file_a"),
            false,
        );
        assert!(p.join("cp_d_2").join("file_a").exists());

        // cleanup
        let _ = fs::remove_dir_all(p.join("cp_d_1"));
        let _ = fs::remove_dir_all(p.join("cp_d_2"));
    }
}
