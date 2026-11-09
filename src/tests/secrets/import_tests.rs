#[cfg(test)]
mod tests {
    use crate::config::settings::SettingsSource;
    use crate::secrets::{import, SecretStore, TOKEN};
    use crate::tests::secrets::{test_dir, FailingStore, MemoryStore};
    use std::fs;

    const CONFIG: &str = "debug = false

# Written by the installer
[openaev]
url = \"http://localhost:3001\"
token = \"config-token\"
unsecured_certificate = false
with_proxy = false
installation_mode = \"service\"
service_name = \"OAEVAgentService\"
tenant_id = \"tenant\"

[other]
token = \"not-the-agent-token\"
";

    #[test]
    fn test_import_moves_the_token_and_removes_only_its_line() {
        // -- PREPARE --
        let dir = test_dir("import-move");
        let config = dir.join("config.toml");
        fs::write(&config, CONFIG).unwrap();
        let store = MemoryStore::default();

        // -- EXECUTE --
        import(&store, "config-token", &config).unwrap();

        // -- ASSERT --
        assert_eq!(store.get(TOKEN).unwrap().as_str(), "config-token");
        assert_eq!(
            fs::read_to_string(&config).unwrap(),
            CONFIG.replace("token = \"config-token\"\n", "")
        );
    }

    #[test]
    fn test_import_replaces_the_stored_token_after_a_reinstall() {
        // -- PREPARE --
        // A reinstall or an upgrade writes a new token in the config: it wins.
        let dir = test_dir("import-reinstall");
        let config = dir.join("config.toml");
        fs::write(&config, CONFIG).unwrap();
        let store = MemoryStore::with_token("old-token");

        // -- EXECUTE --
        import(store.as_ref(), "config-token", &config).unwrap();

        // -- ASSERT --
        assert_eq!(store.get(TOKEN).unwrap().as_str(), "config-token");
    }

    #[test]
    fn test_import_keeps_the_config_token_when_the_store_fails() {
        // -- PREPARE --
        let dir = test_dir("import-store-fails");
        let config = dir.join("config.toml");
        fs::write(&config, CONFIG).unwrap();

        // -- EXECUTE --
        let result = import(&FailingStore, "config-token", &config);

        // -- ASSERT --
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Reinstall the agent"), "{err}");
        assert_eq!(fs::read_to_string(&config).unwrap(), CONFIG);
    }

    #[test]
    fn test_import_fails_when_the_config_cannot_be_rewritten() {
        // -- PREPARE --
        // The token is stored, but the config is not valid TOML any more.
        let dir = test_dir("import-config-invalid");
        let config = dir.join("config.toml");
        let broken = CONFIG.replace("\"config-token\"", "\"config-token");
        fs::write(&config, &broken).unwrap();
        let store = MemoryStore::default();

        // -- EXECUTE --
        let result = import(&store, "config-token", &config);

        // -- ASSERT --
        let err = result.unwrap_err().to_string();
        assert!(err.contains("still in clear text"), "{err}");
        assert!(
            !err.contains("config-token"),
            "the error must not hold the token: {err}"
        );
        assert_eq!(store.get(TOKEN).unwrap().as_str(), "config-token");
    }

    #[test]
    fn test_import_keeps_a_windows_config_readable() {
        // -- PREPARE --
        // The Windows installers write CRLF line endings.
        let dir = test_dir("import-crlf");
        let config = dir.join("agent-config.toml");
        fs::write(&config, CONFIG.replace('\n', "\r\n")).unwrap();
        let store = MemoryStore::default();

        // -- EXECUTE --
        import(&store, "config-token", &config).unwrap();

        // -- ASSERT --
        let settings = SettingsSource::File(dir.join("agent-config"))
            .load()
            .unwrap();
        assert!(settings.openaev.token.is_none());
        assert_eq!(settings.openaev.tenant_id, "tenant");
    }
}
