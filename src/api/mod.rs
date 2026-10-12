use crate::common::error_model::Error;
use crate::secrets::{SecretStore, TOKEN};
use reqwest::header::HeaderValue;
use std::sync::Arc;
use std::time::Duration;
use zeroize::Zeroizing;

mod manage_jobs;
mod register_agent;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const AUTHORIZATION_HEADER: &str = "Authorization";

pub struct Client {
    http_client: reqwest::blocking::Client,
    server_url: String,
    secrets: Arc<dyn SecretStore>,
}

impl Client {
    pub fn new(
        server_url: String,
        secrets: Arc<dyn SecretStore>,
        unsecured_certificate: bool,
        with_proxy: bool,
    ) -> Client {
        let mut http_client = reqwest::blocking::Client::builder()
            .use_rustls_tls()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(5))
            .user_agent(format!("openaev-agent/{VERSION}"));
        // reqwest 0.13's `rustls` feature trusts the OS store only; add the bundled Mozilla roots
        // back so trust stays what it was under 0.12's rustls-tls + rustls-tls-native-roots.
        for root in webpki_root_certs::TLS_SERVER_ROOT_CERTS {
            if let Ok(cert) = reqwest::Certificate::from_der(root.as_ref()) {
                http_client = http_client.add_root_certificate(cert);
            }
        }
        if !with_proxy {
            http_client = http_client.no_proxy();
        }
        if unsecured_certificate {
            http_client = http_client.danger_accept_invalid_certs(true);
        }

        // Remove trailing slash
        let mut url = server_url;
        if url.ends_with('/') {
            url.pop();
        }
        // Initiate client
        Client {
            http_client: http_client.build().unwrap(),
            server_url: url,
            secrets,
        }
    }

    /// Reads the token from the store for this request only, so it does not stay in memory
    /// between requests.
    fn authorization(&self) -> Result<HeaderValue, Error> {
        let token = self.secrets.get(TOKEN)?;
        let bearer = Zeroizing::new(format!("Bearer {}", token.as_str()));
        let mut value =
            HeaderValue::from_str(&bearer).map_err(|err| Error::Internal(err.to_string()))?;
        // Keeps it out of reqwest's debug output.
        value.set_sensitive(true);
        Ok(value)
    }

    #[cfg(test)]
    pub fn get(&self, route: &str) -> Result<reqwest::blocking::RequestBuilder, Error> {
        let api_route = format!("{}{}", self.server_url, route);
        Ok(self
            .http_client
            .get(&api_route)
            .header(AUTHORIZATION_HEADER, self.authorization()?))
    }

    pub fn post(&self, route: &str) -> Result<reqwest::blocking::RequestBuilder, Error> {
        let api_route = format!("{}{}", self.server_url, route);
        Ok(self
            .http_client
            .post(&api_route)
            .header(AUTHORIZATION_HEADER, self.authorization()?))
    }

    pub fn delete(&self, route: &str) -> Result<reqwest::blocking::RequestBuilder, Error> {
        let api_route = format!("{}{}", self.server_url, route);
        Ok(self
            .http_client
            .delete(&api_route)
            .header(AUTHORIZATION_HEADER, self.authorization()?))
    }
}
