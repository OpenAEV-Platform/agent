use std::fs;
use std::fs::{File, OpenOptions};
use std::io;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

/// Replaces the content of `path` with `content`, without ever leaving it half-written.
///
/// How it works: we write the new content to a temporary file next to `path` and flush it to
/// disk. Then the temporary file takes the place of `path` in a single step. If anything
/// fails on the way (disk full, missing rights), `path` keeps its old content. On Windows,
/// if the swap fails halfway, the old file is put back from a backup (see `swap`).
///
/// The new file also keeps the permissions of the old one (owner and mode on Unix, ACL on
/// Windows). This matters because the installers restricted who can read the file.
pub(crate) fn replace_file(path: &Path, content: &[u8]) -> io::Result<()> {
    let temp = sibling(path, "tmp");
    // A temporary file may be left over from an interrupted run. We never reuse it, because
    // we cannot trust what is inside or who can read it.
    remove_if_exists(&temp)?;
    let result = write_temp(path, &temp, content).and_then(|()| swap(path, &temp));
    // Never delete the temporary file while `path` is missing: it may be the only copy left.
    if result.is_err() && path.exists() {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// Puts a file back at `path` when an interrupted replacement left it missing (for example, a
/// crash during ReplaceFileW, before `restore` ran). Returns the file used, if any.
///
/// The backup comes first: it is the old file, with its own ACL. The temporary file is the
/// last resort: it has the new content, but maybe not the old ACL.
pub(crate) fn recover(path: &Path) -> Option<PathBuf> {
    if path.exists() {
        return None;
    }
    [sibling(path, "bak"), sibling(path, "tmp")]
        .into_iter()
        .find(|source| fs::rename(source, path).is_ok())
}

/// Deletes the backup an interrupted replacement left next to `path`. It holds the old
/// content, which can be a secret. Kept while `path` is missing: it is then the only copy.
pub(crate) fn remove_backup(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    remove_if_exists(&sibling(path, "bak"))
}

/// `path` with `extension` added, for example `config.toml.tmp`.
fn sibling(path: &Path, extension: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(extension);
    path.with_file_name(name)
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(err) if err.kind() != ErrorKind::NotFound => Err(err),
        _ => Ok(()),
    }
}

fn write_temp(path: &Path, temp: &Path, content: &[u8]) -> io::Result<()> {
    let mut file = create_temp(path, temp)?;
    file.write_all(content)?;
    file.sync_all()
}

#[cfg(unix)]
fn create_temp(path: &Path, temp: &Path) -> io::Result<File> {
    use std::os::unix::fs::{fchown, MetadataExt, OpenOptionsExt, PermissionsExt};

    let metadata = fs::metadata(path)?;
    // The file is created readable by its creator only. Then it gets the same owner and mode
    // as the file it replaces. All of this happens before we write anything into it.
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(temp)?;
    fchown(&file, Some(metadata.uid()), Some(metadata.gid()))?;
    file.set_permissions(fs::Permissions::from_mode(metadata.mode() & 0o777))?;
    Ok(file)
}

#[cfg(unix)]
fn swap(path: &Path, temp: &Path) -> io::Result<()> {
    fs::rename(temp, path)?;
    // A rename is only saved for sure once the folder that holds the file is flushed to disk.
    let dir = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    File::open(dir)?.sync_all()
}

#[cfg(windows)]
fn create_temp(_path: &Path, temp: &Path) -> io::Result<File> {
    // For a short time, the temporary file has the folder's permissions. ReplaceFileW then
    // gives it the permissions of the file it replaces. This is fine because callers never
    // write a secret here: they write content that has already had its secrets removed.
    OpenOptions::new().write(true).create_new(true).open(temp)
}

#[cfg(windows)]
fn swap(path: &Path, temp: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;
    use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;

    // ReplaceFileW is not atomic: it can fail after `path` was moved away. With a backup
    // file, the old file is kept under that name instead of being deleted, so it can be put
    // back. Without one, ERROR_UNABLE_TO_MOVE_REPLACEMENT leaves no file at `path`.
    let backup = sibling(path, "bak");
    // A backup left by an interrupted run holds the old content. Delete it, unless `path` is
    // missing: then the backup may be the only copy, and restore() puts it back below.
    if path.exists() {
        remove_if_exists(&backup)?;
    }

    let wide = |path: &Path| -> Vec<u16> { path.as_os_str().encode_wide().chain([0]).collect() };
    let (replaced, replacement, backup_name) = (wide(path), wide(temp), wide(&backup));
    // ReplaceFileW copies the ACL and attributes of the old file onto the new one. If it
    // cannot copy them, it fails instead of silently dropping them.
    // SAFETY: the three paths end with a NUL character and live until the call returns. The
    // reserved parameters are allowed to be null.
    let ok = unsafe {
        ReplaceFileW(
            replaced.as_ptr(),
            replacement.as_ptr(),
            backup_name.as_ptr(),
            0,
            ptr::null(),
            ptr::null(),
        )
    };
    if ok == 0 {
        let err = io::Error::last_os_error();
        restore(path);
        return Err(err);
    }

    // The backup holds the old content, which can be a secret (the token in the config).
    if let Err(err) = fs::remove_file(&backup) {
        log::error!(
            "Could not delete {}: {err}. It holds the old content of {}: delete it by hand",
            backup.display(),
            path.display()
        );
    }
    Ok(())
}

/// After a failed ReplaceFileW, puts a file back at `path` if the call left it missing.
#[cfg(windows)]
fn restore(path: &Path) {
    if path.exists() {
        // Nothing was moved, or the failure happened before. Any backup is only a copy.
        let _ = fs::remove_file(sibling(path, "bak"));
        return;
    }
    if recover(path).is_none() {
        log::error!(
            "Could not restore {}: its content is in {} or {}",
            path.display(),
            sibling(path, "bak").display(),
            sibling(path, "tmp").display()
        );
    }
}
