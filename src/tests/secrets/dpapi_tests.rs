#[cfg(test)]
#[cfg(windows)]
mod tests {
    use crate::secrets::{import, DpapiStore, SecretStore, BOOTSTRAP_FILE, TOKEN};
    use crate::tests::secrets::test_dir;
    use std::fs;

    #[test]
    fn test_dpapi_store_reads_back_what_it_wrote() {
        // -- PREPARE --
        let store = DpapiStore::new(test_dir("dpapi-round-trip"));

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
        let store = DpapiStore::new(test_dir("dpapi-encrypted"));

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
        let store = DpapiStore::new(test_dir("dpapi-missing"));

        // -- EXECUTE & ASSERT --
        assert!(store.get(TOKEN).is_err(), "no file");
        fs::write(store.path(TOKEN), b"").unwrap();
        assert!(
            store.get(TOKEN).is_err(),
            "empty file created by the installer"
        );
    }

    #[test]
    fn test_dpapi_store_rejects_a_tampered_file() {
        // -- PREPARE --
        let store = DpapiStore::new(test_dir("dpapi-tampered"));
        store.set(TOKEN, "token").unwrap();
        let mut blob = fs::read(store.path(TOKEN)).unwrap();
        let last = blob.len() - 1;
        blob[last] ^= 0xff;
        fs::write(store.path(TOKEN), blob).unwrap();

        // -- EXECUTE & ASSERT --
        assert!(store.get(TOKEN).is_err());
    }

    #[test]
    fn test_dpapi_store_keeps_the_file_the_installer_created() {
        // -- PREPARE --
        let dir = test_dir("dpapi-in-place");
        let store = DpapiStore::new(dir.clone());
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
        let store = DpapiStore::new(dir);

        // -- EXECUTE --
        import(&store, &bootstrap, None, &config).unwrap();

        // -- ASSERT --
        assert_eq!(store.get(TOKEN).unwrap().as_str(), "bootstrap-token");
        assert!(!bootstrap.exists());
    }
}
