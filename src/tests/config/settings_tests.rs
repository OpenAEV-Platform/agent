#[cfg(test)]
mod tests {
    use crate::config::settings::OpenAEV;
    use zeroize::Zeroizing;

    #[test]
    fn test_debug_output_redacts_the_token() {
        // -- PREPARE --
        let settings = OpenAEV {
            url: "http://localhost:3001".to_string(),
            token: Zeroizing::new("secret-token".to_string()),
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
        let token: Zeroizing<String> = settings.token;
        assert_eq!(token.as_str(), "secret-token");
    }

    #[test]
    fn test_token_is_required() {
        // -- PREPARE --
        let json = r#"{"url": "http://localhost:3001",
            "unsecured_certificate": false, "with_proxy": false,
            "installation_mode": "service", "service_name": "OAEVAgentService",
            "tenant_id": "tenant"}"#;

        // -- EXECUTE --
        let result = serde_json::from_str::<OpenAEV>(json);

        // -- ASSERT --
        assert!(result.is_err());
    }
}
