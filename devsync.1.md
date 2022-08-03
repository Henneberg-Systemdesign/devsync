% devsync(1)
% Jochen Henneberg (jh@henneberg-systemdesign.com)
% August, 2022

[comment]: # (generate man file with 'pandoc devsync.1.md -s -t man > devsync.1')

# NAME

devsync – Synchronization/backup tool for developers

# SYNOPSIS

**devsync** [**options**]

# DESCRIPTION

**devsync** is a backup and synchronization tool with focus on
developers directories. It recognizes several specific directory types
and adjust the backup strategy accordingly. So **devsync** will not do
a by-file backup but try to do clever backups of only that files or
data which is necessary to restore the original content. The rationale
is to save storage space and speed up the backup process.

**devsync** will create a '.devsync' session file in the target
directory which saves all current options about the sync session. If
started in a directory with session file all options are read from
there if none is given otherwise the session file is updated.

# GENERAL OPTIONS

**-h**, **\-\-help**
:   Display usage hints.

**-s**, **\-\-source** DIR
:   Source directory where to start backup.

**-t**, **\-\-target** DIR
:   Target directory to backup into.

**-d**, **\-\-delete**
:   Remove extraneous files and directories.

**-a**, **\-\-archive**
:   Preserve timestamps, permissions and ownership.

**-o**, **\-\-owned**
:   Backup only files and directories that are owned by us.

**-u**, **\-\-ui**
:   Show terminal UI.

**-i**, **\-\-ignore** PATH,PATH,...
:   Comma separated list of file/directory names to ignore, the values
    are matched with 'ends_with()'.

**-j**, **\-\-jobs** NUM
: Number of concurrent sync/backup jobs. This defaults to 10 and is
  extremly helpful with flash drivers but may reduce performance on
  hard drives.

# DIRECTORY CATEGORY 'SPECIAL':
## Yocto

Yocto directories are identified by the existence of directories
'bitbake', 'scripts' and something starting with 'meta'. Once a Yocto
directory has been detected the subdirectories are not scanned for new
types.

**\-\-yocto-ignore**
:   Do not backup Yocto directories.

**\-\-yocto-downloads-sync**
:   Backup the 'downloads' directory, by default this is ignored.

**\-\-yocto-build-sync**
:   Backup the build directories 'build' or 'BUILD', 'cache',
    sstate-cache' and 'buildhistory'.

## Sysroot

Sysroot directories are identified by the existence of directories
'dev', 'usr', 'var' and 'bin'. By default they are ignored. Once a
sysroot directory has been detected the subdirectories are not scanned
for new types.

**\-\-sysroot-sync**
:   Enable backup for sysroot directories.

# DIRECTORY CATEGORY 'BUILD':
## Cargo

Cargo build directories are identified by the file 'CACHEDIR.TAG'. By
default they are completely ignored.

**\-\-cargo-sync**
:   Backup cargo build directories.

## CMake

CMake build directories are identified by the file 'CMakeCache.txt'. By
default they are completely ignored.

**\-\-cmake-sync**
:   Backup CMake build directories.

## Flutter

Flutter build directories are identified by a file ending with
'.cache.dill.track.dill'. By default they are completely ignored.

**\-\-flutter-sync**
:   Backup Flutter build directories.

## Meson

Meson build directories are identified by the files 'meson-info',
'meson-logs' and 'meson-private'. By default they are completely
ignored.

**\-\-meson-sync**
:   Backup Meson build directories.

## Ninja

Ninja build directories are identified by the file 'build.ninja'. By
default they are completely ignored.

**\-\-ninja-sync**
:   Backup Ninja build directories.

# DIRECTORY CATEGORY 'REPOSITORY':
## Subversion

Subversion directories are identified by the directory '.svn'. By
default SVN directories are fully synced and subdirectories are
scanned for other categories.

**\-\-svn-ignore**
:   Ignore SVN directories.

## Git

Git repositories are identified by the directory '.git'. By default a
git repository is synced by checking for stashes which are saved in
the target directory 'stashes', for untracked files which are saved in
the target directory 'untracked' and for unstaged files which are
saved in the target directory 'unstaged'.  
Then **devsync** scans all local branches and if one of them does not
have a matching upstream branch the repository is cloned 'bare' into
the subdirectory 'repo'.  
For git repositories the **-d** flag is ignored, old content is always
removed.

**\-\-git-ignore**
:   Ignore git directories.

**\-\-git-full**
:   Do a full sync - treat repository like a plain directory and
    rescan subdirectories e. g. for build directories.

**\-\-git-ignore-stashes**
:   Do not backup stashes.

**\-\-git-ignore-unstaged**
:   Do not backup unstaged files.

**\-\-git-ignore-untracked**
:   Do not backup untracked files.

**\-\-git-ignore-unpushed**
:   Do not clone bare repository if upstream branches to not match
    local branches.

## Plain - Simple

The default handler. No options, it simply sync all files and
directories but keeps scanning for other categories when processing
subdirectories.

# ENVIRONMENT

You can enable log output (only makes sense if **-u** is not set)
using RUST_LOG environment variable.

# REPORTING BUGS

Bugs can be reported on
<https://github.com/Henneberg-Systemdesign/devsync>, License GPLv3+:
GNU GPL version 3 or later <https://gnu.org/licenses/gpl.html>.

# COPYRIGHT

Copyright © 2022 Jochen Henneberg.
