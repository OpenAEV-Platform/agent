use super::{SecretStore, TOKEN};
use crate::common::error_model::Error;
use crate::config::settings::Settings;
use zeroize::Zeroizing;

/// The token in the config, where the installers write it. Read again on each use, never
/// kept in memory.
pub struct ConfigStore;

impl SecretStore for ConfigStore {
    fn get(&self, name: &str) -> Result<Zeroizing<String>, Error> {
        if name != TOKEN {
            return Err(Error::Internal(format!("no secret named {name}")));
        }
        let settings = Settings::new().map_err(|_| {
            Error::Internal("Unable to read token from agent configuration".to_string())
        })?;
        Ok(settings.openaev.token)
    }
}
