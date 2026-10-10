use std::fs;
use std::fs::{File, OpenOptions};
use std::io;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

/// Replaces the content of `path` with `content`, never leaving it half-written.
///
/// The content is written and synced to a temporary file next to `path`, which then takes
/// its place in one step: if anything fails (full disk, missing rights), `path` keeps its
/// previous content. The new file keeps the permissions of the one it replaces (owner and
/// mode on Unix, ACL on Windows), which the installers restricted.
pub(crate) fn replace_file(path: &Path, content: &[u8]) -> io::Result<()> {
    let temp = temp_path(path);
    // Left over by an interrupted run: never reused, it could hold anything.
    remove_if_exists(&temp)?;
    let result = write_temp(path, &temp, content).and_then(|()| swap(path, &temp));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
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
    // Readable by its creator only, then given the owner and mode of the file it replaces,
    // all before anything is written to it.
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
    // The rename is only durable once the directory holding it is synced.
    let dir = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    File::open(dir)?.sync_all()
}

#[cfg(windows)]
fn create_temp(_path: &Path, temp: &Path) -> io::Result<File> {
    // Gets the folder's ACL until ReplaceFileW gives it the one of the file it replaces.
    // It never holds a secret: callers write content they removed the secrets from.
    OpenOptions::new().write(true).create_new(true).open(temp)
}

#[cfg(windows)]
fn swap(path: &Path, temp: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;
    use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;

    let wide = |path: &Path| -> Vec<u16> { path.as_os_str().encode_wide().chain([0]).collect() };
    let (replaced, replacement) = (wide(path), wide(temp));
    // ReplaceFileW gives the new file the ACL and attributes of the one it replaces, and
    // fails rather than dropping them. Without a backup file, a failure leaves `path` as it
    // was.
    // SAFETY: both paths are NUL-terminated and alive for the whole call; the backup file
    // and the reserved parameters may be null.
    let ok = unsafe {
        ReplaceFileW(
            replaced.as_ptr(),
            replacement.as_ptr(),
            ptr::null(),
            0,
            ptr::null(),
            ptr::null(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
