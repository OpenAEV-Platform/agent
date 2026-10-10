#[cfg(test)]
mod tests {
    use crate::secrets::replace_file;
    use crate::tests::secrets::test_dir;
    use std::fs;

    #[test]
    fn test_replace_file_writes_the_content_and_leaves_no_temporary_file() {
        // -- PREPARE --
        let dir = test_dir("replace-content");
        let path = dir.join("config.toml");
        fs::write(&path, "old content").unwrap();

        // -- EXECUTE --
        replace_file(&path, b"new content").unwrap();

        // -- ASSERT --
        assert_eq!(fs::read_to_string(&path).unwrap(), "new content");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
    }

    #[test]
    fn test_replace_file_ignores_a_temporary_file_left_by_an_interrupted_run() {
        // -- PREPARE --
        let dir = test_dir("replace-leftover");
        let path = dir.join("config.toml");
        fs::write(&path, "old content").unwrap();
        fs::write(dir.join("config.toml.tmp"), "leftover").unwrap();

        // -- EXECUTE --
        replace_file(&path, b"new content").unwrap();

        // -- ASSERT --
        assert_eq!(fs::read_to_string(&path).unwrap(), "new content");
        assert!(!dir.join("config.toml.tmp").exists());
    }

    #[cfg(unix)]
    #[test]
    fn test_replace_file_keeps_the_mode() {
        use std::os::unix::fs::PermissionsExt;

        // -- PREPARE --
        let dir = test_dir("replace-mode");
        let path = dir.join("config.toml");
        fs::write(&path, "old content").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();

        // -- EXECUTE --
        replace_file(&path, b"new content").unwrap();

        // -- ASSERT --
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);
    }

    #[cfg(windows)]
    fn acl(path: &std::path::Path) -> String {
        let output = std::process::Command::new("icacls")
            .arg(path)
            .output()
            .unwrap();
        assert!(output.status.success());
        // The first line starts with the path; the last one is a summary.
        let text = String::from_utf8_lossy(&output.stdout).replace(&*path.to_string_lossy(), "");
        text.lines()
            .map(str::trim)
            .filter(|line| line.contains(':') && !line.contains("Successfully"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[cfg(windows)]
    #[test]
    fn test_replace_file_keeps_the_restricted_acl() {
        // -- PREPARE --
        let dir = test_dir("replace-acl");
        let path = dir.join("config.toml");
        fs::write(&path, "old content").unwrap();
        // Not inherited from the folder, like the ACL the installers set.
        let status = std::process::Command::new("icacls")
            .arg(&path)
            .args(["/inheritance:r", "/grant:r", "*S-1-5-18:F", "*S-1-5-11:F"])
            .status()
            .unwrap();
        assert!(status.success());
        let before = acl(&path);

        // -- EXECUTE --
        replace_file(&path, b"new content").unwrap();

        // -- ASSERT --
        assert_eq!(fs::read_to_string(&path).unwrap(), "new content");
        assert_eq!(acl(&path), before);
        assert!(
            !before.contains("(I)"),
            "the test ACL should not be inherited"
        );
    }

    #[cfg(windows)]
    #[test]
    fn test_replace_file_keeps_the_old_content_when_the_file_cannot_be_replaced() {
        use std::os::windows::fs::OpenOptionsExt;

        // -- PREPARE --
        let dir = test_dir("replace-locked");
        let path = dir.join("config.toml");
        fs::write(&path, "old content").unwrap();
        // Opened without sharing: nothing else can replace or delete it meanwhile.
        let lock = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();

        // -- EXECUTE --
        let result = replace_file(&path, b"new content");
        drop(lock);

        // -- ASSERT --
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "old content");
        assert!(!dir.join("config.toml.tmp").exists());
    }
}
