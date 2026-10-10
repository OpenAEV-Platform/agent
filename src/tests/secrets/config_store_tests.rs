#[cfg(test)]
mod tests {
    use crate::secrets::{ConfigStore, SecretStore, BOOTSTRAP_FILE, TOKEN};
    use crate::tests::secrets::test_dir;
    use std::fs;

    #[test]
    fn test_config_store_reads_the_bootstrap_file_on_each_use() {
        // -- PREPARE --
        let dir = test_dir("config-store-bootstrap");
        let bootstrap = dir.join(BOOTSTRAP_FILE);
        fs::write(&bootstrap, "first-token\n").unwrap();
        let store = ConfigStore::new(bootstrap.clone());

        // -- EXECUTE & ASSERT --
        assert_eq!(store.get(TOKEN).unwrap().as_str(), "first-token");
        fs::write(&bootstrap, "second-token").unwrap();
        assert_eq!(store.get(TOKEN).unwrap().as_str(), "second-token");
    }

    #[test]
    fn test_config_store_refuses_writes() {
        // -- PREPARE --
        let store = ConfigStore::new(test_dir("config-store-write").join(BOOTSTRAP_FILE));

        // -- EXECUTE & ASSERT --
        assert!(store.set(TOKEN, "token").is_err());
    }

    #[test]
    fn test_config_store_only_holds_the_token() {
        // -- PREPARE --
        let dir = test_dir("config-store-name");
        let bootstrap = dir.join(BOOTSTRAP_FILE);
        fs::write(&bootstrap, "token").unwrap();
        let store = ConfigStore::new(bootstrap);

        // -- EXECUTE & ASSERT --
        assert!(store.get("refresh_token").is_err());
    }
}
