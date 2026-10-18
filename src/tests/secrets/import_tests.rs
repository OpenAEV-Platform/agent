#[cfg(test)]
mod tests {
    use crate::secrets::{import, SecretStore, BOOTSTRAP_FILE, PENDING_FILE, TOKEN};
    use crate::tests::secrets::{
        test_dir, FailingStore, FlakyStore, MemoryStore, MismatchingStore, UnreadableStore,
    };
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
        assert!(!dir.join(PENDING_FILE).exists());
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
    fn test_import_keeps_the_clear_text_copies_when_the_token_cannot_be_read_back() {
        // -- PREPARE --
        let dir = test_dir("import-unreadable");
        let bootstrap = dir.join(BOOTSTRAP_FILE);
        let config = dir.join("config.toml");
        fs::write(&bootstrap, "bootstrap-token").unwrap();
        fs::write(&config, CONFIG).unwrap();
        let store = UnreadableStore::default();

        // -- EXECUTE --
        let result = import(&store, &bootstrap, Some("legacy-token"), &config);

        // -- ASSERT --
        assert!(result.is_err());
        assert_eq!(
            store.writes.load(Ordering::SeqCst),
            1,
            "the write succeeded"
        );
        assert_eq!(fs::read_to_string(&bootstrap).unwrap(), "bootstrap-token");
        assert_eq!(fs::read_to_string(&config).unwrap(), CONFIG);
    }

    #[test]
    fn test_import_does_not_overwrite_a_token_the_store_cannot_read() {
        // -- PREPARE --
        // The store holds a newer token, but its first read fails (store busy at startup).
        let dir = test_dir("import-read-error");
        let config = dir.join("config.toml");
        fs::write(&config, CONFIG).unwrap();
        let store = FlakyStore::with_token("new-token");

        // -- EXECUTE --
        let result = import(
            &store,
            &dir.join(BOOTSTRAP_FILE),
            Some("legacy-token"),
            &config,
        );

        // -- ASSERT --
        assert!(result.is_err());
        assert_eq!(store.inner.writes.load(Ordering::SeqCst), 0);
        assert_eq!(
            store.get(TOKEN).unwrap().as_str(),
            "new-token",
            "the newer token must not be overwritten"
        );
        assert_eq!(fs::read_to_string(&config).unwrap(), CONFIG);
    }

    #[test]
    fn test_import_replaces_a_token_the_store_cannot_read_with_the_bootstrap_one() {
        // -- PREPARE --
        let dir = test_dir("import-read-error-bootstrap");
        let bootstrap = dir.join(BOOTSTRAP_FILE);
        let config = dir.join("config.toml");
        fs::write(&bootstrap, "bootstrap-token").unwrap();
        fs::write(&config, CONFIG).unwrap();
        let store = FlakyStore::with_token("new-token");

        // -- EXECUTE --
        import(&store, &bootstrap, Some("legacy-token"), &config).unwrap();

        // -- ASSERT --
        assert_eq!(store.get(TOKEN).unwrap().as_str(), "bootstrap-token");
        assert!(!bootstrap.exists());
    }

    #[test]
    fn test_import_rewrites_a_token_the_store_cannot_read_while_a_write_is_pending() {
        // -- PREPARE --
        let dir = test_dir("import-read-error-pending");
        let config = dir.join("config.toml");
        fs::write(&config, CONFIG).unwrap();
        fs::write(dir.join(PENDING_FILE), "").unwrap();
        let store = FlakyStore::with_token("other-token");

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
        assert!(!dir.join(PENDING_FILE).exists());
    }

    #[test]
    fn test_import_retries_a_config_import_whose_read_back_differed() {
        // -- PREPARE --
        // No bootstrap file: the config holds the only valid token.
        let dir = test_dir("import-mismatch-retry");
        let bootstrap = dir.join(BOOTSTRAP_FILE);
        let config = dir.join("config.toml");
        fs::write(&config, CONFIG).unwrap();
        let store = MismatchingStore::default();

        // -- EXECUTE --
        // Two starts. After the first one, the store holds an unverified value.
        let first = import(&store, &bootstrap, Some("legacy-token"), &config);
        let second = import(&store, &bootstrap, Some("legacy-token"), &config);

        // -- ASSERT --
        assert!(first.is_err());
        assert!(
            second.is_err(),
            "the unverified stored value must not be trusted"
        );
        assert_eq!(
            store.writes.load(Ordering::SeqCst),
            2,
            "each start retries the write"
        );
        assert_eq!(
            fs::read_to_string(&config).unwrap(),
            CONFIG,
            "the only valid token must be kept"
        );
        assert!(dir.join(PENDING_FILE).exists());
    }

    #[test]
    fn test_import_fails_on_an_unverified_stored_token_with_no_clear_text_token() {
        // -- PREPARE --
        // A write was never verified, and the clear-text copies are gone (deleted by hand).
        let dir = test_dir("import-pending-no-source");
        let config = dir.join("config.toml");
        fs::write(&config, CONFIG_WITHOUT_TOKEN).unwrap();
        fs::write(dir.join(PENDING_FILE), "").unwrap();
        let store = MemoryStore::with_token("unverified-token");

        // -- EXECUTE --
        let result = import(store.as_ref(), &dir.join(BOOTSTRAP_FILE), None, &config);

        // -- ASSERT --
        assert!(
            result.is_err(),
            "the store must not be used with an unverified token"
        );
        assert_eq!(store.writes.load(Ordering::SeqCst), 0);
        assert!(dir.join(PENDING_FILE).exists());
    }

    #[test]
    fn test_import_rewrites_a_stored_token_whose_write_was_never_verified() {
        // -- PREPARE --
        // An earlier start wrote a wrong value, then stopped before the check succeeded.
        let dir = test_dir("import-pending");
        let config = dir.join("config.toml");
        fs::write(&config, CONFIG).unwrap();
        fs::write(dir.join(PENDING_FILE), "").unwrap();
        let store = MemoryStore::with_token("other-token");

        // -- EXECUTE --
        import(
            store.as_ref(),
            &dir.join(BOOTSTRAP_FILE),
            Some("legacy-token"),
            &config,
        )
        .unwrap();

        // -- ASSERT --
        assert_eq!(store.get(TOKEN).unwrap().as_str(), "legacy-token");
        assert_eq!(fs::read_to_string(&config).unwrap(), CONFIG_WITHOUT_TOKEN);
        assert!(!dir.join(PENDING_FILE).exists());
    }

    #[test]
    fn test_import_keeps_the_clear_text_copies_when_the_token_read_back_differs() {
        // -- PREPARE --
        let dir = test_dir("import-mismatch");
        let bootstrap = dir.join(BOOTSTRAP_FILE);
        let config = dir.join("config.toml");
        fs::write(&bootstrap, "bootstrap-token").unwrap();
        fs::write(&config, CONFIG).unwrap();
        let store = MismatchingStore::default();

        // -- EXECUTE --
        let result = import(&store, &bootstrap, Some("legacy-token"), &config);

        // -- ASSERT --
        assert!(result.is_err());
        assert_eq!(
            store.writes.load(Ordering::SeqCst),
            1,
            "the write succeeded"
        );
        assert_eq!(fs::read_to_string(&bootstrap).unwrap(), "bootstrap-token");
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
    fn test_import_keeps_the_stored_token_over_an_older_config_token() {
        // -- PREPARE --
        // Config restored from a backup after the token was replaced.
        let dir = test_dir("import-stale-config");
        let config = dir.join("config.toml");
        fs::write(&config, CONFIG).unwrap();
        let store = MemoryStore::with_token("new-token");

        // -- EXECUTE --
        import(
            store.as_ref(),
            &dir.join(BOOTSTRAP_FILE),
            Some("legacy-token"),
            &config,
        )
        .unwrap();

        // -- ASSERT --
        assert_eq!(
            store.get(TOKEN).unwrap().as_str(),
            "new-token",
            "the config token must never replace a token already stored"
        );
        assert_eq!(store.writes.load(Ordering::SeqCst), 0);
        assert_eq!(
            fs::read_to_string(&config).unwrap(),
            CONFIG_WITHOUT_TOKEN,
            "the ignored token must not stay in clear text"
        );
    }

    #[test]
    fn test_import_replaces_the_stored_token_with_the_bootstrap_one() {
        // -- PREPARE --
        let dir = test_dir("import-replace");
        let bootstrap = dir.join(BOOTSTRAP_FILE);
        let config = dir.join("config.toml");
        fs::write(&bootstrap, "newest-token").unwrap();
        fs::write(&config, CONFIG).unwrap();
        let store = MemoryStore::with_token("new-token");

        // -- EXECUTE --
        import(store.as_ref(), &bootstrap, Some("legacy-token"), &config).unwrap();

        // -- ASSERT --
        assert_eq!(store.get(TOKEN).unwrap().as_str(), "newest-token");
        assert!(!bootstrap.exists());
        assert_eq!(fs::read_to_string(&config).unwrap(), CONFIG_WITHOUT_TOKEN);
    }

    #[test]
    fn test_import_replaces_an_empty_stored_token_with_the_config_one() {
        // -- PREPARE --
        let dir = test_dir("import-empty-store");
        let config = dir.join("config.toml");
        fs::write(&config, CONFIG).unwrap();
        let store = MemoryStore::with_token("");

        // -- EXECUTE --
        import(
            store.as_ref(),
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

    /// Opens `path` without sharing: nothing else can read, replace or delete it meanwhile.
    #[cfg(windows)]
    fn lock(path: &std::path::Path) -> fs::File {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(path)
            .unwrap()
    }

    #[cfg(windows)]
    #[test]
    fn test_import_retries_the_config_clean_up_while_the_file_is_locked() {
        // -- PREPARE --
        let dir = test_dir("import-locked-briefly");
        let bootstrap = dir.join(BOOTSTRAP_FILE);
        let config = dir.join("config.toml");
        fs::write(&bootstrap, "new-token").unwrap();
        fs::write(&config, CONFIG).unwrap();
        let store = MemoryStore::default();
        // Locked for a moment only, like an antivirus scanning a file.
        let config_lock = lock(&config);
        let unlock = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            drop(config_lock);
        });

        // -- EXECUTE --
        import(&store, &bootstrap, Some("legacy-token"), &config).unwrap();
        unlock.join().unwrap();

        // -- ASSERT --
        assert_eq!(store.get(TOKEN).unwrap().as_str(), "new-token");
        assert_eq!(fs::read_to_string(&config).unwrap(), CONFIG_WITHOUT_TOKEN);
        assert!(!bootstrap.exists());
    }

    #[cfg(windows)]
    #[test]
    fn test_import_keeps_the_bootstrap_file_while_the_config_stays_locked() {
        // -- PREPARE --
        let dir = test_dir("import-locked");
        let bootstrap = dir.join(BOOTSTRAP_FILE);
        let config = dir.join("config.toml");
        fs::write(&bootstrap, "new-token").unwrap();
        fs::write(&config, CONFIG).unwrap();
        let store = MemoryStore::default();
        let config_lock = lock(&config);

        // -- EXECUTE --
        let result = import(&store, &bootstrap, Some("legacy-token"), &config);
        drop(config_lock);

        // -- ASSERT --
        assert!(result.is_ok(), "the token is safe in the store");
        assert_eq!(store.get(TOKEN).unwrap().as_str(), "new-token");
        assert_eq!(fs::read_to_string(&config).unwrap(), CONFIG);
        assert!(bootstrap.exists(), "the newer token must still win");
    }

    #[test]
    fn test_import_deletes_a_backup_left_with_the_token_after_a_restart() {
        // -- PREPARE --
        // Crash after the cleaned config was installed, before its backup was deleted.
        let dir = test_dir("import-leftover-backup");
        let config = dir.join("config.toml");
        let backup = dir.join("config.toml.bak");
        fs::write(&config, CONFIG_WITHOUT_TOKEN).unwrap();
        fs::write(&backup, CONFIG).unwrap();
        let store = MemoryStore::with_token("legacy-token");

        // -- EXECUTE --
        import(store.as_ref(), &dir.join(BOOTSTRAP_FILE), None, &config).unwrap();

        // -- ASSERT --
        assert!(!backup.exists(), "the backup holds the token in clear text");
        assert_eq!(fs::read_to_string(&config).unwrap(), CONFIG_WITHOUT_TOKEN);
        assert_eq!(store.writes.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_import_keeps_the_backup_while_the_store_has_no_token() {
        // -- PREPARE --
        let dir = test_dir("import-backup-no-store");
        let config = dir.join("config.toml");
        let backup = dir.join("config.toml.bak");
        fs::write(&config, CONFIG_WITHOUT_TOKEN).unwrap();
        fs::write(&backup, CONFIG).unwrap();
        let store = MemoryStore::default();

        // -- EXECUTE --
        import(&store, &dir.join(BOOTSTRAP_FILE), None, &config).unwrap();

        // -- ASSERT --
        assert!(backup.exists(), "the backup is the only copy of the token");
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
