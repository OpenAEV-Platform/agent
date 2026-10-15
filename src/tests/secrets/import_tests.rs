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

    /// Imports `legacy-token` from a config holding `content`, and returns the config after.
    fn import_config(name: &str, content: &str) -> String {
        let dir = test_dir(name);
        let config = dir.join("config.toml");
        fs::write(&config, content).unwrap();
        let store = MemoryStore::default();
        import(
            &store,
            &dir.join(BOOTSTRAP_FILE),
            Some("legacy-token"),
            &config,
        )
        .unwrap();
        assert_eq!(store.get(TOKEN).unwrap().as_str(), "legacy-token");
        fs::read_to_string(&config).unwrap()
    }

    #[test]
    fn test_import_removes_only_the_token_of_the_openaev_table() {
        // -- PREPARE --
        let content = "token = \"root\"\n\n[openaev]\ntoken_file = \"x\"\ntokens = 2\n  token = \"legacy-token\" # from the installer\n\n[other]\ntoken = \"kept\"\n";

        // -- EXECUTE --
        let after = import_config("import-scoped", content);

        // -- ASSERT --
        assert_eq!(
            after,
            "token = \"root\"\n\n[openaev]\ntoken_file = \"x\"\ntokens = 2\n\n[other]\ntoken = \"kept\"\n"
        );
    }

    #[test]
    fn test_import_removes_a_quoted_token_key() {
        // -- PREPARE --
        let content = "[openaev]\nurl = \"http://localhost:3001\"\n\"token\" = \"legacy-token\"\n";

        // -- EXECUTE --
        let after = import_config("import-quoted", content);

        // -- ASSERT --
        assert_eq!(after, "[openaev]\nurl = \"http://localhost:3001\"\n");
    }

    #[test]
    fn test_import_removes_a_dotted_token_key() {
        // -- PREPARE --
        let content = "openaev.url = \"http://localhost:3001\"\nopenaev.token = \"legacy-token\"\n";

        // -- EXECUTE --
        let after = import_config("import-dotted", content);

        // -- ASSERT --
        assert_eq!(after, "openaev.url = \"http://localhost:3001\"\n");
    }

    #[test]
    fn test_import_leaves_a_config_it_cannot_parse_as_it_was() {
        // -- PREPARE --
        let content = "[openaev]\ntoken = \"legacy-token\"\nbroken =\n";

        // -- EXECUTE --
        let after = import_config("import-invalid", content);

        // -- ASSERT --
        assert_eq!(
            after, content,
            "a config that cannot be parsed is not touched"
        );
    }

    /// A config that still holds `token` but cannot be rewritten (it does not parse).
    fn stuck_config(token: &str) -> String {
        format!("[openaev]\ntoken = \"{token}\"\nbroken =\n")
    }

    #[test]
    fn test_import_keeps_the_bootstrap_file_while_the_config_holds_an_older_token() {
        // -- PREPARE --
        let dir = test_dir("import-stuck-config");
        let bootstrap = dir.join(BOOTSTRAP_FILE);
        let config = dir.join("config.toml");
        fs::write(&bootstrap, "new-token").unwrap();
        fs::write(&config, stuck_config("old-token")).unwrap();
        let store = MemoryStore::default();

        // -- EXECUTE --
        // Two starts: the config token cannot be removed either time.
        import(&store, &bootstrap, Some("old-token"), &config).unwrap();
        import(&store, &bootstrap, Some("old-token"), &config).unwrap();

        // -- ASSERT --
        assert_eq!(
            store.get(TOKEN).unwrap().as_str(),
            "new-token",
            "the older config token must never replace the bootstrap one"
        );
        assert!(bootstrap.exists(), "the bootstrap file still has to win");
        assert_eq!(store.writes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_import_deletes_the_bootstrap_file_when_the_config_holds_the_same_token() {
        // -- PREPARE --
        let dir = test_dir("import-stuck-same");
        let bootstrap = dir.join(BOOTSTRAP_FILE);
        let config = dir.join("config.toml");
        fs::write(&bootstrap, "same-token").unwrap();
        fs::write(&config, stuck_config("same-token")).unwrap();
        let store = MemoryStore::default();

        // -- EXECUTE --
        import(&store, &bootstrap, Some("same-token"), &config).unwrap();

        // -- ASSERT --
        assert_eq!(store.get(TOKEN).unwrap().as_str(), "same-token");
        assert!(
            !bootstrap.exists(),
            "the config token cannot overwrite anything"
        );
    }

    #[test]
    fn test_import_finishes_a_clean_up_interrupted_after_the_config() {
        // -- PREPARE --
        // Stopped after the config was cleaned, before the bootstrap file was deleted.
        let dir = test_dir("import-interrupted");
        let bootstrap = dir.join(BOOTSTRAP_FILE);
        let config = dir.join("config.toml");
        fs::write(&bootstrap, "new-token").unwrap();
        fs::write(&config, CONFIG_WITHOUT_TOKEN).unwrap();
        let store = MemoryStore::with_token("new-token");

        // -- EXECUTE --
        import(store.as_ref(), &bootstrap, None, &config).unwrap();

        // -- ASSERT --
        assert_eq!(store.writes.load(Ordering::SeqCst), 0);
        assert!(!bootstrap.exists());
        assert_eq!(fs::read_to_string(&config).unwrap(), CONFIG_WITHOUT_TOKEN);
    }
}
