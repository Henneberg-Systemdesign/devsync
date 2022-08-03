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
