const TOKEN: &str = "token";

#[cfg(test)]
mod tests {
    use crate::api::{Client, AUTHORIZATION_HEADER};
    use crate::tests::api::client::TOKEN;
    use mockito;
    use std::env;

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
        let bad_ssl_url = "https://self-signed.badssl.com/";

        let client_without_unsecured_certificate =
            Client::new(bad_ssl_url.to_string(), TOKEN.to_string(), false, false);
        let client_with_unsecured_certificate =
            Client::new(bad_ssl_url.to_string(), TOKEN.to_string(), true, true);

        // -- EXECUTE --
        let res_without_unsecured_certificate = client_without_unsecured_certificate.get("").send();
        let res_with_unsecured_certificate = client_with_unsecured_certificate.get("").send();

        // -- ASSERT --
        assert!(
            res_without_unsecured_certificate.is_err(),
            "Client should not bypass the bad ssl"
        );
        assert!(
            res_with_unsecured_certificate.is_ok(),
            "Client should bypass the bad ssl"
        );
    }
}
