use super::{missing, read_bootstrap, SecretStore, TOKEN};
use crate::common::error_model::Error;
use crate::config::settings::Settings;
use std::path::PathBuf;
use zeroize::Zeroizing;

/// Reads the token from where the installers left it in clear text: the bootstrap file if
/// there is one, otherwise the config file.
///
/// The agent uses this store when the platform has no supported OS secret store yet, or when
/// moving the token there failed. The token is read again each time it is needed, and never
/// kept in memory.
pub struct ConfigStore {
    bootstrap: PathBuf,
    /// The config file, without its extension. `None` means the development settings are
    /// used.
    config_file: Option<PathBuf>,
}

impl ConfigStore {
    pub fn new(bootstrap: PathBuf, config_file: Option<PathBuf>) -> Self {
        ConfigStore {
            bootstrap,
            config_file,
        }
    }
}

impl SecretStore for ConfigStore {
    fn get(&self, name: &str) -> Result<Zeroizing<String>, Error> {
        if name != TOKEN {
            return Err(missing(format!("no secret named {name}")));
        }
        if let Some(token) = read_bootstrap(&self.bootstrap)? {
            return Ok(token);
        }
        let settings = match &self.config_file {
            Some(config_file) => Settings::from_file(config_file),
            None => Settings::new(),
        }
        .map_err(|_| {
            Error::Internal("Unable to read token from agent configuration".to_string())
        })?;
        settings
            .openaev
            .token
            .ok_or_else(|| missing("no token in the agent config".to_string()))
    }

    fn set(&self, _name: &str, _value: &str) -> Result<(), Error> {
        Err(Error::Internal(
            "the agent does not write secrets to its config".to_string(),
        ))
    }
}
