#[cfg(test)]
mod tests {
    use crate::config::settings::OpenAEV;

    #[test]
    fn test_debug_output_redacts_the_token() {
        // -- PREPARE --
        let settings = OpenAEV {
            url: "http://localhost:3001".to_string(),
            token: Some("secret-token".to_string()),
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
}
