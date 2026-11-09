use super::{SecretStore, TOKEN};
use crate::common::error_model::Error;
use crate::config::settings::SettingsSource;
use config::ConfigError;
use log::error;
use std::io;
use zeroize::Zeroizing;

/// Secret store backed by the agent config file. The token is read from the file on each call
/// and is never cached. This store is used in development and on platforms without an OS
/// secret store.
pub struct ConfigStore {
    source: SettingsSource,
}

impl ConfigStore {
    pub fn new(source: SettingsSource) -> Self {
        ConfigStore { source }
    }
}

impl SecretStore for ConfigStore {
    fn get(&self, name: &str) -> Result<Zeroizing<String>, Error> {
        if name != TOKEN {
            return Err(Error::Internal(format!("no secret named {name}")));
        }
        let settings = self.source.load().map_err(|err| {
            let message = format!(
                "Unable to read token from agent configuration: {}",
                reason(&err)
            );
            error!("{message}");
            Error::Internal(message)
        })?;
        // Without an OS secret store, the config file must hold the token.
        settings
            .openaev
            .token
            .ok_or_else(|| Error::Internal("no token in the agent config".to_string()))
    }

    // The agent never writes the token to the config file: it would be in clear text again.
    fn set(&self, _name: &str, _value: &str) -> Result<(), Error> {
        Err(Error::Internal(
            "the agent does not write secrets to its config".to_string(),
        ))
    }
}

/// Says why the config could not be read, using only the error kind and the setting name.
/// The error's own text is not used: it can contain the token (a parse error quotes the failing
/// line, a type error quotes the value).
pub(crate) fn reason(err: &ConfigError) -> String {
    match err {
        ConfigError::Foreign(cause) => match cause.downcast_ref::<io::Error>().map(io::Error::kind)
        {
            Some(io::ErrorKind::NotFound) => "the config file is missing".to_string(),
            Some(io::ErrorKind::PermissionDenied) => {
                "access to the config file is denied".to_string()
            }
            _ => "the config file cannot be read".to_string(),
        },
        ConfigError::FileParse { .. } => "the config file is not valid".to_string(),
        ConfigError::NotFound(key) => format!("the setting `{key}` is missing"),
        ConfigError::Type { key: Some(key), .. } => {
            format!("the setting `{key}` has the wrong type")
        }
        ConfigError::At { error, .. } => reason(error),
        _ => "a setting is invalid".to_string(),
    }
}
