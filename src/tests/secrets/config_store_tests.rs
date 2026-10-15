#[cfg(test)]
mod tests {
    use crate::secrets::{ConfigStore, SecretStore, TOKEN};
    use crate::tests::secrets::test_dir;
    use std::fs;
    use std::path::{Path, PathBuf};

    /// A config like the one the installers write, holding `token`.
    fn config(token: &str) -> String {
        format!(
            "debug = false\n\n[openaev]\nurl = \"http://localhost:3001\"\ntoken = \"{token}\"\n\
             unsecured_certificate = false\nwith_proxy = false\ninstallation_mode = \"service\"\n\
             service_name = \"OAEVAgentService\"\ntenant_id = \"tenant\"\n"
        )
    }

    /// A store reading `agent-config.toml` in `dir`, given without its extension as in
    /// production.
    fn store_in(dir: &Path) -> (ConfigStore, PathBuf) {
        let store = ConfigStore::new(Some(dir.join("agent-config")));
        (store, dir.join("agent-config.toml"))
    }

    #[test]
    fn test_config_store_only_holds_the_token() {
        // -- PREPARE --
        let (store, file) = store_in(&test_dir("config-store-name"));
        fs::write(&file, config("first-token")).unwrap();

        // -- EXECUTE & ASSERT --
        assert!(store.get("refresh_token").is_err());
    }

    #[test]
    fn test_config_store_reads_the_new_token_after_an_edit() {
        // -- PREPARE --
        let (store, file) = store_in(&test_dir("config-store-edit"));
        fs::write(&file, config("first-token")).unwrap();

        // -- EXECUTE --
        let before = store.get(TOKEN).unwrap();
        fs::write(&file, config("second-token")).unwrap();
        let after = store.get(TOKEN).unwrap();

        // -- ASSERT --
        // Same store, no restart: the config is read again on each use.
        assert_eq!(before.as_str(), "first-token");
        assert_eq!(after.as_str(), "second-token");
    }

    #[test]
    fn test_config_store_fails_without_a_config_and_recovers_once_it_exists() {
        // -- PREPARE --
        let (store, file) = store_in(&test_dir("config-store-missing"));

        // -- EXECUTE --
        let missing = store.get(TOKEN);
        fs::write(&file, config("first-token")).unwrap();
        let recovered = store.get(TOKEN);

        // -- ASSERT --
        assert!(missing.is_err());
        assert_eq!(recovered.unwrap().as_str(), "first-token");
    }

    #[test]
    fn test_config_store_fails_on_an_invalid_config_and_recovers_after_repair() {
        // -- PREPARE --
        let (store, file) = store_in(&test_dir("config-store-invalid"));
        // The closing quote is missing on the token line itself.
        let broken = config("leaked-token").replace("leaked-token\"", "leaked-token");
        let without_token = config("first-token").replace("token = \"first-token\"\n", "");

        // -- EXECUTE --
        fs::write(&file, broken).unwrap();
        let invalid = store.get(TOKEN);
        fs::write(&file, without_token).unwrap();
        let no_token = store.get(TOKEN);
        fs::write(&file, config("first-token")).unwrap();
        let repaired = store.get(TOKEN);

        // -- ASSERT --
        let Err(invalid) = invalid else {
            panic!("an invalid config must not give a token");
        };
        // The raw parse error quotes the failing line, here the token: it must not reach the
        // error message.
        assert!(!invalid.to_string().contains("leaked-token"));
        assert!(no_token.is_err());
        assert_eq!(repaired.unwrap().as_str(), "first-token");
    }
}
