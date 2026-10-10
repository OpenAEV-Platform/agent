#[cfg(test)]
mod tests {
    use crate::secrets::{import, SecretStore, BOOTSTRAP_FILE, TOKEN};
    use crate::tests::secrets::{test_dir, FailingStore, MemoryStore};
    use std::fs;
    use std::sync::atomic::Ordering;

    const CONFIG: &str = "debug=false\r\n\r\n[openaev]\r\nurl = \"http://localhost:3001\"\r\ntoken = \"legacy-token\"\r\ntenant_id = \"tenant\"\r\n";
    const CONFIG_WITHOUT_TOKEN: &str =
        "debug=false\r\n\r\n[openaev]\r\nurl = \"http://localhost:3001\"\r\ntenant_id = \"tenant\"\r\n";

    #[test]
    fn test_import_moves_the_bootstrap_token_and_deletes_the_file() {
        // -- PREPARE --
        let dir = test_dir("import-bootstrap");
        let bootstrap = dir.join(BOOTSTRAP_FILE);
        let config = dir.join("config.toml");
        fs::write(&bootstrap, "bootstrap-token\r\n").unwrap();
        fs::write(&config, CONFIG_WITHOUT_TOKEN).unwrap();
        let store = MemoryStore::default();

        // -- EXECUTE --
        import(&store, &bootstrap, None, &config).unwrap();

        // -- ASSERT --
        assert_eq!(store.get(TOKEN).unwrap().as_str(), "bootstrap-token");
        assert!(
            !bootstrap.exists(),
            "the clear-text bootstrap file should be deleted"
        );
        assert_eq!(fs::read_to_string(&config).unwrap(), CONFIG_WITHOUT_TOKEN);
    }

    #[test]
    fn test_import_moves_the_config_token_and_removes_only_its_line() {
        // -- PREPARE --
        let dir = test_dir("import-config");
        let config = dir.join("config.toml");
        fs::write(&config, CONFIG).unwrap();
        let store = MemoryStore::default();

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
        assert_eq!(fs::read_to_string(&config).unwrap(), CONFIG_WITHOUT_TOKEN);
    }

    #[test]
    fn test_import_prefers_the_bootstrap_token_and_cleans_both_copies() {
        // -- PREPARE --
        let dir = test_dir("import-both");
        let bootstrap = dir.join(BOOTSTRAP_FILE);
        let config = dir.join("config.toml");
        fs::write(&bootstrap, "new-token").unwrap();
        fs::write(&config, CONFIG).unwrap();
        let store = MemoryStore::default();

        // -- EXECUTE --
        import(&store, &bootstrap, Some("legacy-token"), &config).unwrap();

        // -- ASSERT --
        assert_eq!(store.get(TOKEN).unwrap().as_str(), "new-token");
        assert!(!bootstrap.exists());
        assert_eq!(fs::read_to_string(&config).unwrap(), CONFIG_WITHOUT_TOKEN);
    }

    #[test]
    fn test_import_keeps_the_clear_text_copies_when_the_store_fails() {
        // -- PREPARE --
        let dir = test_dir("import-failure");
        let bootstrap = dir.join(BOOTSTRAP_FILE);
        let config = dir.join("config.toml");
        fs::write(&bootstrap, "bootstrap-token").unwrap();
        fs::write(&config, CONFIG).unwrap();

        // -- EXECUTE --
        let result = import(&FailingStore, &bootstrap, Some("legacy-token"), &config);

        // -- ASSERT --
        assert!(result.is_err());
        assert!(bootstrap.exists(), "the token must not be lost");
        assert_eq!(fs::read_to_string(&config).unwrap(), CONFIG);
    }

    #[test]
    fn test_import_does_not_rewrite_a_token_already_stored() {
        // -- PREPARE --
        let dir = test_dir("import-idempotent");
        let config = dir.join("config.toml");
        fs::write(&config, CONFIG).unwrap();
        let store = MemoryStore::with_token("legacy-token");

        // -- EXECUTE --
        import(
            store.as_ref(),
            &dir.join(BOOTSTRAP_FILE),
            Some("legacy-token"),
            &config,
        )
        .unwrap();

        // -- ASSERT --
        assert_eq!(store.writes.load(Ordering::SeqCst), 0);
        assert_eq!(fs::read_to_string(&config).unwrap(), CONFIG_WITHOUT_TOKEN);
    }

    #[test]
    fn test_import_without_any_token_changes_nothing() {
        // -- PREPARE --
        let dir = test_dir("import-nothing");
        let config = dir.join("config.toml");
        fs::write(&config, CONFIG_WITHOUT_TOKEN).unwrap();
        let store = MemoryStore::default();

        // -- EXECUTE --
        import(&store, &dir.join(BOOTSTRAP_FILE), None, &config).unwrap();

        // -- ASSERT --
        assert_eq!(store.writes.load(Ordering::SeqCst), 0);
        assert!(store.get(TOKEN).is_err());
        assert_eq!(fs::read_to_string(&config).unwrap(), CONFIG_WITHOUT_TOKEN);
    }

    #[test]
    fn test_import_keeps_keys_that_only_start_with_token() {
        // -- PREPARE --
        let dir = test_dir("import-similar-keys");
        let config = dir.join("config.toml");
        let content = "token_file = \"x\"\ntokens = 2\n  token = \"legacy-token\"\n";
        fs::write(&config, content).unwrap();
        let store = MemoryStore::default();

        // -- EXECUTE --
        import(
            &store,
            &dir.join(BOOTSTRAP_FILE),
            Some("legacy-token"),
            &config,
        )
        .unwrap();

        // -- ASSERT --
        assert_eq!(
            fs::read_to_string(&config).unwrap(),
            "token_file = \"x\"\ntokens = 2\n"
        );
    }
}
