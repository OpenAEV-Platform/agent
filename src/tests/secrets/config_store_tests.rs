#[cfg(test)]
mod tests {
    use crate::config::settings::SettingsSource;
    use crate::secrets::config_store::reason;
    use crate::secrets::{ConfigStore, SecretStore, BOOTSTRAP_FILE, TOKEN};
    use crate::tests::secrets::test_dir;
    use config::ConfigError;
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};

    /// A config like the one the installers write, holding `token`.
    fn config(token: &str) -> String {
        format!(
            "debug = false\n\n[openaev]\nurl = \"http://localhost:3001\"\ntoken = \"{token}\"\n\
             unsecured_certificate = false\nwith_proxy = false\ninstallation_mode = \"service\"\n\
             service_name = \"OAEVAgentService\"\ntenant_id = \"tenant\"\n"
        )
    }

    /// A store reading the bootstrap file and `agent-config.toml` in `dir`, the config given
    /// without its extension as in production. Returns the store, the config and the
    /// bootstrap file.
    fn store_in(dir: &Path) -> (ConfigStore, PathBuf, PathBuf) {
        let bootstrap = dir.join(BOOTSTRAP_FILE);
        let store = ConfigStore::new(
            bootstrap.clone(),
            SettingsSource::File(dir.join("agent-config")),
        );
        (store, dir.join("agent-config.toml"), bootstrap)
    }

    #[test]
    fn test_config_store_reads_the_bootstrap_file_on_each_use() {
        // -- PREPARE --
        let dir = test_dir("config-store-bootstrap");
        let (store, _, bootstrap) = store_in(&dir);
        fs::write(&bootstrap, "first-token\n").unwrap();

        // -- EXECUTE & ASSERT --
        assert_eq!(store.get(TOKEN).unwrap().as_str(), "first-token");
        fs::write(&bootstrap, "second-token").unwrap();
        assert_eq!(store.get(TOKEN).unwrap().as_str(), "second-token");
    }

    #[test]
    fn test_config_store_prefers_the_bootstrap_file_to_the_config() {
        // -- PREPARE --
        let dir = test_dir("config-store-bootstrap-first");
        let (store, file, bootstrap) = store_in(&dir);
        fs::write(&file, config("config-token")).unwrap();
        fs::write(&bootstrap, "bootstrap-token").unwrap();

        // -- EXECUTE & ASSERT --
        assert_eq!(store.get(TOKEN).unwrap().as_str(), "bootstrap-token");
        fs::remove_file(&bootstrap).unwrap();
        assert_eq!(store.get(TOKEN).unwrap().as_str(), "config-token");
    }

    #[test]
    fn test_config_store_refuses_writes() {
        // -- PREPARE --
        let dir = test_dir("config-store-write");
        let (store, _, _) = store_in(&dir);

        // -- EXECUTE & ASSERT --
        assert!(store.set(TOKEN, "first-token").is_err());
    }

    #[test]
    fn test_config_store_only_holds_the_token() {
        // -- PREPARE --
        let dir = test_dir("config-store-name");
        let (store, file, _) = store_in(&dir);
        fs::write(&file, config("first-token")).unwrap();

        // -- EXECUTE & ASSERT --
        assert!(store.get("refresh_token").is_err());
    }

    #[test]
    fn test_config_store_reads_the_new_token_after_an_edit() {
        // -- PREPARE --
        let dir = test_dir("config-store-edit");
        let (store, file, _) = store_in(&dir);
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
        let dir = test_dir("config-store-missing");
        let (store, file, _) = store_in(&dir);

        // -- EXECUTE --
        let missing = store.get(TOKEN);
        fs::write(&file, config("first-token")).unwrap();
        let recovered = store.get(TOKEN);

        // -- ASSERT --
        let Err(missing) = missing else {
            panic!("a missing config must not give a token");
        };
        assert!(missing.to_string().contains("the config file is missing"));
        assert_eq!(recovered.unwrap().as_str(), "first-token");
    }

    #[test]
    fn test_config_store_fails_on_an_invalid_config_and_recovers_after_repair() {
        // -- PREPARE --
        let dir = test_dir("config-store-invalid");
        let (store, file, _) = store_in(&dir);
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
        let invalid = invalid.to_string();
        assert!(!invalid.contains("leaked-token"), "{invalid}");
        // It still says why, so that the log is useful.
        assert!(
            invalid.contains("the config file is not valid"),
            "{invalid}"
        );
        let Err(no_token) = no_token else {
            panic!("a config without a token must not give one");
        };
        let no_token = no_token.to_string();
        assert!(
            no_token.contains("no token in the agent config"),
            "{no_token}"
        );
        assert_eq!(repaired.unwrap().as_str(), "first-token");
    }

    #[test]
    fn test_config_store_names_a_setting_of_the_wrong_type_without_its_value() {
        // -- PREPARE --
        let dir = test_dir("config-store-type");
        let (store, file, _) = store_in(&dir);
        let wrong_type = config("first-token").replace(
            "unsecured_certificate = false",
            "unsecured_certificate = \"leaked-value\"",
        );
        fs::write(&file, wrong_type).unwrap();

        // -- EXECUTE --
        let Err(err) = store.get(TOKEN) else {
            panic!("a setting of the wrong type must not give a token");
        };

        // -- ASSERT --
        // The raw type error quotes the value it found, and that value could be a secret.
        let message = err.to_string();
        assert!(!message.contains("leaked-value"), "{message}");
        assert!(
            message.contains("the setting `openaev.unsecured_certificate` has the wrong type"),
            "{message}"
        );
    }

    #[test]
    fn test_reason_tells_a_denied_access_from_a_missing_file() {
        // -- PREPARE --
        // A denied read is hard to set up the same way on every OS, so the errors are built here.
        let denied =
            ConfigError::Foreign(Box::new(io::Error::from(io::ErrorKind::PermissionDenied)));
        let missing = ConfigError::Foreign(Box::new(io::Error::from(io::ErrorKind::NotFound)));

        // -- EXECUTE & ASSERT --
        assert_eq!(reason(&denied), "access to the config file is denied");
        assert_eq!(reason(&missing), "the config file is missing");
    }
}
