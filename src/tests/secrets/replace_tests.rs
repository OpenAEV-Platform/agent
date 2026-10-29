#[cfg(test)]
mod tests {
    use crate::config::settings::SettingsSource;
    use crate::secrets::{recover, replace_file};
    use crate::tests::secrets::test_dir;
    use std::fs;

    const CONFIG: &str = "debug = false\n\n[openaev]\nurl = \"http://localhost:3001\"\n\
         unsecured_certificate = false\nwith_proxy = false\ninstallation_mode = \"service\"\n\
         service_name = \"OAEVAgentService\"\ntenant_id = \"tenant\"\n";

    #[test]
    fn test_settings_load_after_recovering_a_config_left_in_its_backup() {
        // -- PREPARE --
        // Crash during ReplaceFileW: the config was moved to the backup, restore never ran.
        let dir = test_dir("recover-startup");
        let path = dir.join("agent-config.toml");
        fs::write(dir.join("agent-config.toml.bak"), CONFIG).unwrap();
        assert!(SettingsSource::File(dir.join("agent-config"))
            .load()
            .is_err());

        // -- EXECUTE --
        // What main() does at startup, before loading the settings.
        let source = recover(&path);
        let settings = SettingsSource::File(dir.join("agent-config")).load();

        // -- ASSERT --
        assert_eq!(source, Some(dir.join("agent-config.toml.bak")));
        assert_eq!(settings.unwrap().openaev.url, "http://localhost:3001");
        assert!(!dir.join("agent-config.toml.bak").exists());
    }

    #[test]
    fn test_recover_uses_the_temporary_file_when_no_backup_is_left() {
        // -- PREPARE --
        let dir = test_dir("recover-temp");
        let path = dir.join("config.toml");
        fs::write(dir.join("config.toml.tmp"), "new content").unwrap();

        // -- EXECUTE --
        let source = recover(&path);

        // -- ASSERT --
        assert_eq!(source, Some(dir.join("config.toml.tmp")));
        assert_eq!(fs::read_to_string(&path).unwrap(), "new content");
    }

    #[test]
    fn test_recover_leaves_an_existing_config_and_its_backup_alone() {
        // -- PREPARE --
        let dir = test_dir("recover-nothing");
        let path = dir.join("config.toml");
        fs::write(&path, "current content").unwrap();
        fs::write(dir.join("config.toml.bak"), "old content").unwrap();

        // -- EXECUTE --
        let source = recover(&path);

        // -- ASSERT --
        assert_eq!(source, None);
        assert_eq!(fs::read_to_string(&path).unwrap(), "current content");
        // Deleting the backup is up to import(), once the token is safe in the store.
        assert!(dir.join("config.toml.bak").exists());
    }

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
        assert_eq!(fs::read_dir(&*dir).unwrap().count(), 1);
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
        assert!(!dir.join("config.toml.bak").exists());
    }

    #[cfg(windows)]
    #[test]
    fn test_replace_file_deletes_a_backup_left_by_an_interrupted_run() {
        // -- PREPARE --
        let dir = test_dir("replace-leftover-backup");
        let path = dir.join("config.toml");
        fs::write(&path, "old content").unwrap();
        fs::write(dir.join("config.toml.bak"), "older content with a token").unwrap();

        // -- EXECUTE --
        replace_file(&path, b"new content").unwrap();

        // -- ASSERT --
        assert_eq!(fs::read_to_string(&path).unwrap(), "new content");
        assert!(
            !dir.join("config.toml.bak").exists(),
            "the backup can hold a secret"
        );
    }

    #[cfg(windows)]
    #[test]
    fn test_replace_file_puts_the_old_file_back_from_the_backup() {
        // -- PREPARE --
        // State left by ERROR_UNABLE_TO_MOVE_REPLACEMENT_2: the old file was moved to the
        // backup, and nothing is left at `path`.
        let dir = test_dir("replace-restore-backup");
        let path = dir.join("config.toml");
        fs::write(dir.join("config.toml.bak"), "old content").unwrap();

        // -- EXECUTE --
        let result = replace_file(&path, b"new content");

        // -- ASSERT --
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "old content");
        assert!(!dir.join("config.toml.bak").exists());
        assert!(!dir.join("config.toml.tmp").exists());
    }

    #[cfg(windows)]
    #[test]
    fn test_replace_file_puts_the_new_content_in_place_when_no_backup_is_left() {
        // -- PREPARE --
        // State left by ERROR_UNABLE_TO_MOVE_REPLACEMENT without a backup: nothing at `path`,
        // only the temporary file survives.
        let dir = test_dir("replace-restore-temp");
        let path = dir.join("config.toml");

        // -- EXECUTE --
        let result = replace_file(&path, b"new content");

        // -- ASSERT --
        assert!(result.is_err());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "new content",
            "the temporary file must not be deleted while it is the only copy"
        );
        assert!(!dir.join("config.toml.tmp").exists());
    }
}
