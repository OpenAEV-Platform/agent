#[cfg(test)]
#[cfg(windows)]
mod tests {
    use crate::secrets::{import, DpapiStore, SecretStore, TOKEN};
    use crate::tests::secrets::test_dir;
    use std::fs;

    #[test]
    fn test_dpapi_store_reads_back_what_it_wrote() {
        // -- PREPARE --
        let dir = test_dir("dpapi-round-trip");
        let store = DpapiStore::new(dir.to_path_buf());

        // -- EXECUTE --
        store.set(TOKEN, "first-token").unwrap();
        let first = store.get(TOKEN).unwrap();
        store.set(TOKEN, "second-token").unwrap();
        let second = store.get(TOKEN).unwrap();

        // -- ASSERT --
        assert_eq!(first.as_str(), "first-token");
        assert_eq!(second.as_str(), "second-token");
    }

    #[test]
    fn test_dpapi_store_does_not_write_the_token_in_clear_text() {
        // -- PREPARE --
        let dir = test_dir("dpapi-encrypted");
        let store = DpapiStore::new(dir.to_path_buf());

        // -- EXECUTE --
        store.set(TOKEN, "clear-text-token").unwrap();

        // -- ASSERT --
        let blob = fs::read(store.path(TOKEN)).unwrap();
        let needle = b"clear-text-token";
        assert!(!blob.windows(needle.len()).any(|window| window == needle));
    }

    #[test]
    fn test_dpapi_store_fails_without_a_stored_token() {
        // -- PREPARE --
        let dir = test_dir("dpapi-missing");
        let store = DpapiStore::new(dir.to_path_buf());

        // -- EXECUTE --
        let result = store.get(TOKEN);

        // -- ASSERT --
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Reinstall the agent"), "{err}");
    }

    #[test]
    fn test_dpapi_store_rejects_a_tampered_file() {
        // -- PREPARE --
        let dir = test_dir("dpapi-tampered");
        let store = DpapiStore::new(dir.to_path_buf());
        store.set(TOKEN, "token").unwrap();
        let mut blob = fs::read(store.path(TOKEN)).unwrap();
        let last = blob.len() - 1;
        blob[last] ^= 0xff;
        fs::write(store.path(TOKEN), blob).unwrap();

        // -- EXECUTE & ASSERT --
        assert!(store.get(TOKEN).is_err());
    }

    /// The permissions of `path`, as `icacls` prints them, without the path itself.
    fn acl(path: &std::path::Path) -> String {
        let output = std::process::Command::new("icacls")
            .arg(path)
            .output()
            .unwrap();
        assert!(output.status.success());
        let text = String::from_utf8_lossy(&output.stdout).replace(&*path.to_string_lossy(), "");
        text.lines()
            .map(str::trim)
            .filter(|line| line.contains(':') && !line.contains("Successfully"))
            .collect::<Vec<_>>()
            .join(
                "
",
            )
    }

    #[test]
    fn test_dpapi_store_creates_a_missing_file_with_restricted_permissions() {
        // -- PREPARE --
        // No .dpapi file yet.
        let dir = test_dir("dpapi-created");
        let store = DpapiStore::new(dir.to_path_buf());

        // -- EXECUTE --
        store.set(TOKEN, "token").unwrap();

        // -- ASSERT --
        assert_eq!(store.get(TOKEN).unwrap().as_str(), "token");
        let acl = acl(&store.path(TOKEN));
        assert!(
            !acl.contains("(I)"),
            "nothing inherited from the folder: {acl}"
        );
        assert!(acl.contains(r"NT AUTHORITY\SYSTEM:(F)"), "{acl}");
        assert!(acl.contains(r"BUILTIN\Administrators:(F)"), "{acl}");
        for other in ["Authenticated Users", r"BUILTIN\Users", "Everyone"] {
            assert!(!acl.contains(other), "{other} has access: {acl}");
        }
    }

    #[test]
    fn test_dpapi_store_rewrites_an_existing_file_in_place() {
        // -- PREPARE --
        let dir = test_dir("dpapi-in-place");
        let store = DpapiStore::new(dir.to_path_buf());
        fs::write(store.path(TOKEN), b"").unwrap();
        let created = fs::metadata(store.path(TOKEN)).unwrap().created().unwrap();

        // -- EXECUTE --
        store.set(TOKEN, "token").unwrap();

        // -- ASSERT --
        let metadata = fs::metadata(store.path(TOKEN)).unwrap();
        assert_eq!(
            metadata.created().unwrap(),
            created,
            "rewritten, not replaced"
        );
        assert!(metadata.len() > 0);
    }

    #[test]
    fn test_import_moves_the_config_token_into_dpapi() {
        // -- PREPARE --
        let dir = test_dir("dpapi-import");
        let config = dir.join("config.toml");
        fs::write(
            &config,
            "[openaev]
token = \"config-token\"
",
        )
        .unwrap();
        let store = DpapiStore::new(dir.to_path_buf());

        // -- EXECUTE --
        import(&store, Some("config-token"), &config).unwrap();

        // -- ASSERT --
        assert_eq!(store.get(TOKEN).unwrap().as_str(), "config-token");
        assert_eq!(
            fs::read_to_string(&config).unwrap(),
            "[openaev]
"
        );
    }
}
