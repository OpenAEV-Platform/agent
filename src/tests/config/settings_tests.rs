#[cfg(test)]
mod tests {
    use crate::config::settings::{OpenAEV, SettingsSource};
    use crate::tests::secrets::test_dir;
    use std::fs;
    use zeroize::Zeroizing;

    #[test]
    fn test_debug_output_redacts_the_token() {
        // -- PREPARE --
        let settings = OpenAEV {
            url: "http://localhost:3001".to_string(),
            token: Some(Zeroizing::new("secret-token".to_string())),
            unsecured_certificate: false,
            with_proxy: false,
            installation_mode: "service".to_string(),
            service_name: "OAEVAgentService".to_string(),
            tenant_id: "tenant".to_string(),
        };

        // -- EXECUTE --
        let output = format!("{settings:?}");

        // -- ASSERT --
        assert!(!output.contains("secret-token"));
        assert!(output.contains("<redacted>"));
    }

    #[test]
    fn test_token_is_deserialized_straight_into_a_zeroizing_buffer() {
        // -- PREPARE --
        let json = r#"{"url": "http://localhost:3001", "token": "secret-token",
            "unsecured_certificate": false, "with_proxy": false,
            "installation_mode": "service", "service_name": "OAEVAgentService",
            "tenant_id": "tenant"}"#;

        // -- EXECUTE --
        let settings: OpenAEV = serde_json::from_str(json).unwrap();

        // -- ASSERT --
        let token: Zeroizing<String> = settings.token.unwrap();
        assert_eq!(token.as_str(), "secret-token");
    }

    #[test]
    fn test_token_is_optional() {
        // -- PREPARE --
        let json = r#"{"url": "http://localhost:3001",
            "unsecured_certificate": false, "with_proxy": false,
            "installation_mode": "service", "service_name": "OAEVAgentService",
            "tenant_id": "tenant"}"#;

        // -- EXECUTE --
        let settings: OpenAEV = serde_json::from_str(json).unwrap();

        // -- ASSERT --
        assert!(settings.token.is_none());
    }

    const CONFIG: &str = "debug = false

[openaev]
url = \"http://localhost:3001\"
         unsecured_certificate = false
with_proxy = false
installation_mode = \"service\"
         service_name = \"OAEVAgentService\"
tenant_id = \"tenant\"
";

    #[test]
    fn test_os_store_flag_is_off_by_default() {
        // -- PREPARE --
        // A config written by the installers: no [secrets] table.
        let dir = test_dir("settings-flag-default");
        fs::write(dir.join("agent-config.toml"), CONFIG).unwrap();

        // -- EXECUTE --
        let settings = SettingsSource::File(dir.join("agent-config"))
            .load()
            .unwrap();

        // -- ASSERT --
        assert!(!settings.secrets.os_store);
    }

    #[test]
    fn test_os_store_flag_can_be_turned_on() {
        // -- PREPARE --
        let dir = test_dir("settings-flag-on");
        let config = format!(
            "{CONFIG}
[secrets]
os_store = true
"
        );
        fs::write(dir.join("agent-config.toml"), config).unwrap();

        // -- EXECUTE --
        let settings = SettingsSource::File(dir.join("agent-config"))
            .load()
            .unwrap();

        // -- ASSERT --
        assert!(settings.secrets.os_store);
    }
}
