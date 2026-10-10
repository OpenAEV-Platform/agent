const TOKEN: &str = "token";

#[cfg(test)]
mod tests {
    use crate::api::{Client, AUTHORIZATION_HEADER};
    use crate::tests::api::client::TOKEN;
    use crate::tests::secrets::{FailingStore, MemoryStore};
    use mockito;
    use std::env;
    use std::sync::Arc;

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
        let client = Client::new(server_url, MemoryStore::with_token(TOKEN), false, false);

        // -- EXECUTE & ASSERT --
        let res = client.post("/api/test").unwrap().send();
        assert!(res.is_ok(), "User-Agent should match expected format");
    }

    #[test]
    fn test_client_fails_the_request_without_a_token() {
        // -- PREPARE --
        let client = Client::new(
            "http://localhost".to_string(),
            Arc::new(FailingStore),
            false,
            false,
        );

        // -- EXECUTE & ASSERT --
        assert!(client.post("/api/test").is_err());
    }

    #[test]
    fn test_client_keeps_the_token_out_of_debug_output() {
        // -- PREPARE --
        let client = Client::new(
            "http://localhost".to_string(),
            MemoryStore::with_token(TOKEN),
            false,
            false,
        );

        // -- EXECUTE --
        let request = client.post("/api/test").unwrap().build().unwrap();

        // -- ASSERT --
        let header = request.headers().get(AUTHORIZATION_HEADER).unwrap();
        assert!(header.is_sensitive());
        assert!(!format!("{request:?}").contains(TOKEN));
    }

    #[test]
    fn test_with_proxy_disables_http_proxy() {
        // -- PREPARE --
        env::set_var("HTTP_PROXY", "http://127.0.0.1:9999");

        let mut server = mockito::Server::new();
        let server_url = server.url();
        server.mock("POST", "/api/test").with_status(200).create();
        let client_without_proxy = Client::new(
            server_url.clone(),
            MemoryStore::with_token(TOKEN),
            false,
            false,
        );
        let client_with_proxy = Client::new(
            server_url.clone(),
            MemoryStore::with_token(TOKEN),
            false,
            true,
        );

        // -- EXECUTE --
        let res_without_proxy = client_without_proxy.post("/api/test").unwrap().send();
        let res_with_proxy = client_with_proxy.post("/api/test").unwrap().send();

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
        let bad_ssl_url = "https://self-signed.badssl.com/";

        let client_without_unsecured_certificate = Client::new(
            bad_ssl_url.to_string(),
            MemoryStore::with_token(TOKEN),
            false,
            false,
        );
        let client_with_unsecured_certificate = Client::new(
            bad_ssl_url.to_string(),
            MemoryStore::with_token(TOKEN),
            true,
            true,
        );

        // -- EXECUTE --
        let res_without_unsecured_certificate =
            client_without_unsecured_certificate.get("").unwrap().send();
        let res_with_unsecured_certificate =
            client_with_unsecured_certificate.get("").unwrap().send();

        // -- ASSERT --
        match (
            res_without_unsecured_certificate,
            res_with_unsecured_certificate,
        ) {
            // the unsecured client still completes the handshake, so its failure means the host is down
            (_, Err(err)) => eprintln!("skipped, {bad_ssl_url} is unreachable: {err}"),
            (Err(_), Ok(_)) => {}
            (Ok(_), Ok(_)) => panic!("Client should not bypass the bad ssl"),
        }
    }

    #[test]
    fn test_valid_certificate_is_accepted() {
        // -- PREPARE --
        let valid_ssl_url = "https://sha256.badssl.com/";
        let client = Client::new(
            valid_ssl_url.to_string(),
            MemoryStore::with_token(TOKEN),
            false,
            false,
        );
        let reachability_probe = Client::new(
            valid_ssl_url.to_string(),
            MemoryStore::with_token(TOKEN),
            true,
            false,
        );

        // -- EXECUTE & ASSERT --
        let Err(err) = client.get("").unwrap().send() else {
            return;
        };
        // the probe skips chain validation, so its own failure means the host is down
        match reachability_probe.get("").unwrap().send() {
            Ok(_) => panic!("Client should trust a publicly valid certificate: {err}"),
            Err(probe_err) => eprintln!("skipped, {valid_ssl_url} is unreachable: {probe_err}"),
        }
    }

    #[test]
    #[ignore = "requires the os-trust-store CI job"]
    fn test_os_trust_store_is_consulted() {
        // -- PREPARE --
        let url = env::var("OAEV_OS_TRUST_URL")
            .expect("OAEV_OS_TRUST_URL must be set by the os-trust-store CI job");
        let client = Client::new(url, MemoryStore::with_token(TOKEN), false, false);

        // -- EXECUTE & ASSERT --
        assert!(
            client.get("").unwrap().send().is_ok(),
            "Client should trust a CA present only in the OS trust store"
        );
    }
}
