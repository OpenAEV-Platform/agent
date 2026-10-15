use super::{SecretStore, TOKEN};
use crate::common::error_model::Error;
use crate::config::settings::Settings;
use std::path::PathBuf;
use zeroize::Zeroizing;

/// The token in the config, where the installers write it. Read again on each use, never
/// kept in memory.
pub struct ConfigStore {
    /// The config file, without its extension; `None` reads the development settings.
    config_file: Option<PathBuf>,
}

impl ConfigStore {
    pub fn new(config_file: Option<PathBuf>) -> Self {
        ConfigStore { config_file }
    }
}

impl SecretStore for ConfigStore {
    fn get(&self, name: &str) -> Result<Zeroizing<String>, Error> {
        if name != TOKEN {
            return Err(Error::Internal(format!("no secret named {name}")));
        }
        let settings = match &self.config_file {
            Some(config_file) => Settings::from_file(config_file),
            None => Settings::new(),
        }
        .map_err(|_| {
            Error::Internal("Unable to read token from agent configuration".to_string())
        })?;
        Ok(settings.openaev.token)
    }
}
