# devsync
Backup tool for developers written in the [Rust programming
language](https://www.rust-lang.org/).

## Rationale
**devsync** is a backup and synchronization tool with focus on
developers directories. It recognizes several specific directory types
and adjust the backup strategy accordingly. So **devsync** will not do
a by-file backup but try to do clever backups of only that files or
data which is necessary to restore the original content. The rationale
is to save storage space and speed up the backup process.

## Session cache
**devsync** will create a '.devsync.session' session file in the
target directory which saves all current options about the sync
session. If started in a directory with session file all options are
read from there if none is given otherwise the session file is
updated.

## Log file
**devsync** will create a '.devsync.log log file in the target
directory which log entries for runtime errors as well as for each
skipped directory. The logs are dropped when a new session is started.

## Terminal ui
**devsync** can provide a simple terminal interface when started with
'-u'. The ui has a progress bar which is of limited help because it
only tells you about the number of directories that are already known
and how many have been processed. However, it still moves forward most
of the time and gives at least a hint that something is happening. It
also shows you the backup jobs that are currently running and all
runtime issues that happen along the way.

It will remain active when the backup process has been completed and
waits for a press on 'q' or 'Q' to terminate.

Once the backup process is complete you can navigate through the
runtime log with the up/down and page-up/page-down keys.

## Details
Read the manpage for more information or look at the output of -h.

## Supported directory categories and types
- Special
  - Yocto
  - Sysroot
- Build
  - Cargo
  - CMake
  - Flutter
  - Meson
  - Ninja
- VCS repositories
  - Subversion
  - Git
- Simple - the default
