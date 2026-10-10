use super::{read_bootstrap, SecretStore, TOKEN};
use crate::common::error_model::Error;
use crate::config::settings::Settings;
use std::path::PathBuf;
use zeroize::Zeroizing;

/// The token where the installers leave it in clear text: the bootstrap file when one is
/// waiting, otherwise the config. Used where no OS secret store is supported yet, or when
/// moving the token there failed. Read again on each use, never kept in memory.
pub struct ConfigStore {
    bootstrap: PathBuf,
}

impl ConfigStore {
    pub fn new(bootstrap: PathBuf) -> Self {
        ConfigStore { bootstrap }
    }
}

impl SecretStore for ConfigStore {
    fn get(&self, name: &str) -> Result<Zeroizing<String>, Error> {
        if name != TOKEN {
            return Err(Error::Internal(format!("no secret named {name}")));
        }
        if let Some(token) = read_bootstrap(&self.bootstrap)? {
            return Ok(token);
        }
        let settings = Settings::new().map_err(|err| Error::Internal(err.to_string()))?;
        settings
            .openaev
            .token
            .map(Zeroizing::new)
            .ok_or_else(|| Error::Internal("no token in the agent config".to_string()))
    }

    fn set(&self, _name: &str, _value: &str) -> Result<(), Error> {
        Err(Error::Internal(
            "the agent does not write secrets to its config".to_string(),
        ))
    }
}
