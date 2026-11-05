const TOKEN: &str = "token";

#[cfg(test)]
mod tests {
    use crate::api::{Client, AUTHORIZATION_HEADER};
    use crate::tests::api::client::TOKEN;
    use crate::tests::api::tls_server::serve_self_signed_https;
    use mockito;
    use std::env;
    use std::error::Error;
    use std::io;

    fn is_invalid_certificate(err: &reqwest::Error) -> bool {
        let mut current: Option<&(dyn Error + 'static)> = Some(err);
        while let Some(e) = current {
            if let Some(rustls::Error::InvalidCertificate(_)) = e.downcast_ref::<rustls::Error>() {
                return true;
            }
            // io::Error::source() skips the error it wraps, so step into it explicitly
            current = match e.downcast_ref::<io::Error>().and_then(io::Error::get_ref) {
                Some(inner) => Some(inner),
                None => e.source(),
            };
        }
        false
    }

    #[test]
    fn test_client_headers() {
        // -- PREPARE
        let mut server = mockito::Server::new();
        let server_url = server.url();
        server
            .mock("POST", "/api/test")
            .match_header(
                "user-agent",
                format!("openaev-agent/{}", crate::api::VERSION).as_str(),
            )
            .match_header(AUTHORIZATION_HEADER, format!("Bearer {}", TOKEN).as_str())
            .with_status(200)
            .create();
        let client = Client::new(server_url, TOKEN.to_string(), false, false);

        // -- EXECUTE & ASSERT --
        let res = client.post("/api/test").send();
        assert!(res.is_ok(), "User-Agent should match expected format");
    }

    #[test]
    fn test_with_proxy_disables_http_proxy() {
        // -- PREPARE --
        env::set_var("HTTP_PROXY", "http://127.0.0.1:9999");

        let mut server = mockito::Server::new();
        let server_url = server.url();
        server.mock("POST", "/api/test").with_status(200).create();
        let client_without_proxy = Client::new(server_url.clone(), TOKEN.to_string(), false, false);
        let client_with_proxy = Client::new(server_url.clone(), TOKEN.to_string(), false, true);

        // -- EXECUTE --
        let res_without_proxy = client_without_proxy.post("/api/test").send();
        let res_with_proxy = client_with_proxy.post("/api/test").send();

        // -- ASSERT --
        assert!(res_without_proxy.is_ok(), "Client should bypass the proxy");
        assert!(
            res_with_proxy.is_err(),
            "Client should not bypass the proxy"
        );

        // -- CLEAN --
        env::remove_var("HTTP_PROXY");
    }

    #[test]
    fn test_unsecured_certificate_acceptance() {
        // -- PREPARE --
        let url = serve_self_signed_https();
        let client_without_unsecured_certificate =
            Client::new(url.clone(), TOKEN.to_string(), false, false);
        let client_with_unsecured_certificate = Client::new(url, TOKEN.to_string(), true, false);

        // -- EXECUTE --
        let res_without_unsecured_certificate = client_without_unsecured_certificate.get("").send();
        let res_with_unsecured_certificate = client_with_unsecured_certificate.get("").send();

        // -- ASSERT --
        assert!(
            res_without_unsecured_certificate
                .as_ref()
                .is_err_and(is_invalid_certificate),
            "Client should reject the self-signed certificate: {:?}",
            res_without_unsecured_certificate
        );
        assert!(
            res_with_unsecured_certificate.is_ok(),
            "Client should bypass the bad ssl when unsecured: {:?}",
            res_with_unsecured_certificate.err()
        );
    }

    #[test]
    #[ignore = "requires the os-trust-store CI job"]
    fn test_bundled_roots_are_trusted() {
        // -- PREPARE --
        // the CI job empties the OS trust store, so only the bundled roots can validate this URL
        let url = env::var("OAEV_BUNDLED_ROOTS_URL")
            .expect("OAEV_BUNDLED_ROOTS_URL must be set by the os-trust-store CI job");
        let client = Client::new(url, TOKEN.to_string(), false, false);

        // -- EXECUTE --
        let res = client.get("").send();

        // -- ASSERT --
        assert!(
            res.is_ok(),
            "Client should trust a public certificate through its bundled roots: {:?}",
            res.err()
        );
    }

    #[test]
    #[ignore = "requires the os-trust-store CI job"]
    fn test_os_trust_store_is_consulted() {
        // -- PREPARE --
        let url = env::var("OAEV_OS_TRUST_URL")
            .expect("OAEV_OS_TRUST_URL must be set by the os-trust-store CI job");
        let client = Client::new(url, TOKEN.to_string(), false, false);

        // -- EXECUTE & ASSERT --
        assert!(
            client.get("").send().is_ok(),
            "Client should trust a CA present only in the OS trust store"
        );
    }
}
