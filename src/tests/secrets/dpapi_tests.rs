#[cfg(test)]
#[cfg(windows)]
mod tests {
    use crate::config::settings::SettingsSource;
    use crate::secrets::{
        choose, import, is_missing, ConfigStore, DpapiStore, SecretStore, BOOTSTRAP_FILE, TOKEN,
    };
    use crate::tests::secrets::{test_dir, MemoryStore};
    use std::fs;
    use std::sync::Arc;

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

        // -- EXECUTE & ASSERT --
        // Reported as missing, not as a read error: the import may then write the token.
        assert!(is_missing(&store.get(TOKEN).unwrap_err()), "no file");
        fs::write(store.path(TOKEN), b"").unwrap();
        assert!(is_missing(&store.get(TOKEN).unwrap_err()), "empty file");
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
        // A read error, not a missing token: the import must not overwrite it.
        assert!(!is_missing(&store.get(TOKEN).unwrap_err()));
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
    fn test_import_moves_the_bootstrap_token_into_dpapi() {
        // -- PREPARE --
        let dir = test_dir("dpapi-import");
        let bootstrap = dir.join(BOOTSTRAP_FILE);
        let config = dir.join("config.toml");
        fs::write(&bootstrap, "bootstrap-token").unwrap();
        fs::write(&config, "[openaev]\r\ntenant_id = \"tenant\"\r\n").unwrap();
        let store = DpapiStore::new(dir.to_path_buf());

        // -- EXECUTE --
        import(&store, &bootstrap, None, &config).unwrap();

        // -- ASSERT --
        assert_eq!(store.get(TOKEN).unwrap().as_str(), "bootstrap-token");
        assert!(!bootstrap.exists());
    }

    #[test]
    fn test_import_moves_the_config_token_into_an_empty_dpapi_file() {
        // -- PREPARE --
        // Older install: the token is only in the config.
        let dir = test_dir("dpapi-import-config");
        let config = dir.join("config.toml");
        fs::write(
            &config,
            "[openaev]
token = \"legacy-token\"
",
        )
        .unwrap();
        let store = DpapiStore::new(dir.to_path_buf());
        fs::write(store.path(TOKEN), b"").unwrap();

        // -- EXECUTE --
        import(
            &store,
            &dir.join(BOOTSTRAP_FILE),
            Some("legacy-token"),
            &config,
        )
        .unwrap();

        // -- ASSERT --
        assert_eq!(store.get(TOKEN).unwrap().as_str(), "legacy-token");
        assert_eq!(
            fs::read_to_string(&config).unwrap(),
            "[openaev]
"
        );
    }

    #[test]
    fn test_flag_off_leaves_the_bootstrap_token_out_of_dpapi() {
        // -- PREPARE --
        // Token in a bootstrap file, empty .dpapi file.
        let dir = test_dir("dpapi-flag-off");
        let bootstrap = dir.join(BOOTSTRAP_FILE);
        fs::write(&bootstrap, "bootstrap-token").unwrap();
        let store = Arc::new(DpapiStore::new(dir.to_path_buf()));
        fs::write(store.path(TOKEN), b"").unwrap();

        // -- EXECUTE --
        let chosen = choose(
            store.clone(),
            MemoryStore::with_token("clear-text-token"),
            false,
            &bootstrap,
            None,
            &dir.join("config.toml"),
        );

        // -- ASSERT --
        assert_eq!(chosen.get(TOKEN).unwrap().as_str(), "clear-text-token");
        assert!(bootstrap.exists(), "nothing is deleted");
        assert_eq!(
            fs::metadata(store.path(TOKEN)).unwrap().len(),
            0,
            "nothing is written to DPAPI"
        );
    }

    #[test]
    fn test_flag_off_keeps_reading_the_config_token_after_an_upgrade() {
        // -- PREPARE --
        // Token in the config, as the installers write it. No bootstrap file, no .dpapi file.
        let config_content = "debug = false

[openaev]
url = \"http://localhost:3001\"
             token = \"legacy-token\"
unsecured_certificate = false
with_proxy = false
             installation_mode = \"service\"
service_name = \"OAEVAgentService\"
             tenant_id = \"tenant\"
";
        let dir = test_dir("dpapi-flag-off-upgrade");
        let bootstrap = dir.join(BOOTSTRAP_FILE);
        let config = dir.join("agent-config.toml");
        fs::write(&config, config_content).unwrap();
        let store = Arc::new(DpapiStore::new(dir.to_path_buf()));
        let clear_text = Arc::new(ConfigStore::new(
            bootstrap.clone(),
            SettingsSource::File(dir.join("agent-config")),
        ));

        // -- EXECUTE --
        let chosen = choose(
            store.clone(),
            clear_text,
            false,
            &bootstrap,
            Some("legacy-token"),
            &config,
        );

        // -- ASSERT --
        assert_eq!(
            chosen.get(TOKEN).unwrap().as_str(),
            "legacy-token",
            "the agent keeps working as before the upgrade"
        );
        assert_eq!(
            fs::read_to_string(&config).unwrap(),
            config_content,
            "the config is not modified"
        );
        assert!(!store.path(TOKEN).exists(), "no .dpapi file is created");
        assert!(!bootstrap.exists(), "no bootstrap file is created");
    }
}
