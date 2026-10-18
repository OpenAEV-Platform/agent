#[cfg(test)]
mod tests {
    use crate::secrets::{choose, SecretStore, BOOTSTRAP_FILE, PENDING_FILE, TOKEN};
    use crate::tests::secrets::{test_dir, FailingStore, MemoryStore};
    use std::fs;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    const CONFIG: &str = "[openaev]\nurl = \"http://localhost:3001\"\ntoken = \"config-token\"\n";

    /// The clear-text store, holding `clear-text-token`. Lets each test see which store
    /// `choose` returned.
    fn clear_text() -> Arc<dyn SecretStore> {
        MemoryStore::with_token("clear-text-token")
    }

    #[test]
    fn test_flag_on_moves_the_token_into_the_os_store() {
        // -- PREPARE --
        let dir = test_dir("open-flag-on");
        let bootstrap = dir.join(BOOTSTRAP_FILE);
        fs::write(&bootstrap, "bootstrap-token").unwrap();
        let store = Arc::new(MemoryStore::default());

        // -- EXECUTE --
        let chosen = choose(
            store.clone(),
            clear_text(),
            true,
            &bootstrap,
            None,
            &dir.join("config.toml"),
        );

        // -- ASSERT --
        assert_eq!(chosen.get(TOKEN).unwrap().as_str(), "bootstrap-token");
        assert!(!bootstrap.exists());
    }

    #[test]
    fn test_flag_on_falls_back_to_clear_text_when_the_move_fails() {
        // -- PREPARE --
        let dir = test_dir("open-flag-on-failure");
        let bootstrap = dir.join(BOOTSTRAP_FILE);
        fs::write(&bootstrap, "bootstrap-token").unwrap();

        // -- EXECUTE --
        let chosen = choose(
            Arc::new(FailingStore),
            clear_text(),
            true,
            &bootstrap,
            None,
            &dir.join("config.toml"),
        );

        // -- ASSERT --
        assert_eq!(chosen.get(TOKEN).unwrap().as_str(), "clear-text-token");
        assert!(bootstrap.exists());
    }

    #[test]
    fn test_flag_off_keeps_the_bootstrap_token_in_clear_text() {
        // -- PREPARE --
        // New installer, flag off: the token is only in the bootstrap file.
        let dir = test_dir("open-flag-off-bootstrap");
        let bootstrap = dir.join(BOOTSTRAP_FILE);
        fs::write(&bootstrap, "bootstrap-token").unwrap();
        let store = Arc::new(MemoryStore::default());

        // -- EXECUTE --
        let chosen = choose(
            store.clone(),
            clear_text(),
            false,
            &bootstrap,
            None,
            &dir.join("config.toml"),
        );

        // -- ASSERT --
        assert_eq!(chosen.get(TOKEN).unwrap().as_str(), "clear-text-token");
        assert_eq!(store.writes.load(Ordering::SeqCst), 0, "nothing is moved");
        assert!(bootstrap.exists(), "nothing is deleted");
    }

    #[test]
    fn test_flag_off_keeps_the_config_token_in_clear_text() {
        // -- PREPARE --
        // Older install, flag off: the token is only in the config.
        let dir = test_dir("open-flag-off-config");
        let config = dir.join("config.toml");
        fs::write(&config, CONFIG).unwrap();
        let store = Arc::new(MemoryStore::default());

        // -- EXECUTE --
        let chosen = choose(
            store.clone(),
            clear_text(),
            false,
            &dir.join(BOOTSTRAP_FILE),
            Some("config-token"),
            &config,
        );

        // -- ASSERT --
        assert_eq!(chosen.get(TOKEN).unwrap().as_str(), "clear-text-token");
        assert_eq!(store.writes.load(Ordering::SeqCst), 0);
        assert_eq!(fs::read_to_string(&config).unwrap(), CONFIG);
    }

    #[test]
    fn test_flag_off_still_reads_a_token_moved_while_the_flag_was_on() {
        // -- PREPARE --
        // The flag was on, the token was moved and its clear-text copies deleted. Then the
        // flag was turned off.
        let dir = test_dir("open-flag-off-migrated");
        let store = Arc::new(MemoryStore::default());
        store.set_token("stored-token");

        // -- EXECUTE --
        let chosen = choose(
            store.clone(),
            clear_text(),
            false,
            &dir.join(BOOTSTRAP_FILE),
            None,
            &dir.join("config.toml"),
        );

        // -- ASSERT --
        assert_eq!(
            chosen.get(TOKEN).unwrap().as_str(),
            "stored-token",
            "the agent would be offline with the clear-text store"
        );
    }

    #[test]
    fn test_flag_off_prefers_a_verified_stored_token_to_an_older_config_token() {
        // -- PREPARE --
        // Same, but an older config came back (restored from a backup).
        let dir = test_dir("open-flag-off-stale-config");
        let config = dir.join("config.toml");
        fs::write(&config, CONFIG).unwrap();
        let store = Arc::new(MemoryStore::default());
        store.set_token("stored-token");

        // -- EXECUTE --
        let chosen = choose(
            store.clone(),
            clear_text(),
            false,
            &dir.join(BOOTSTRAP_FILE),
            Some("config-token"),
            &config,
        );

        // -- ASSERT --
        assert_eq!(chosen.get(TOKEN).unwrap().as_str(), "stored-token");
        assert_eq!(fs::read_to_string(&config).unwrap(), CONFIG);
    }

    #[test]
    fn test_flag_off_does_not_read_a_stored_token_whose_write_was_never_verified() {
        // -- PREPARE --
        let dir = test_dir("open-flag-off-pending");
        fs::write(dir.join(PENDING_FILE), "").unwrap();
        let store = Arc::new(MemoryStore::default());
        store.set_token("unverified-token");

        // -- EXECUTE --
        let chosen = choose(
            store.clone(),
            clear_text(),
            false,
            &dir.join(BOOTSTRAP_FILE),
            None,
            &dir.join("config.toml"),
        );

        // -- ASSERT --
        assert_eq!(chosen.get(TOKEN).unwrap().as_str(), "clear-text-token");
    }
}
