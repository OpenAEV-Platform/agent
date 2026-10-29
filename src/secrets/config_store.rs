use super::{missing, read_bootstrap, SecretStore, TOKEN};
use crate::common::error_model::Error;
use crate::config::settings::SettingsSource;
use config::ConfigError;
use std::io;
use std::path::PathBuf;
use zeroize::Zeroizing;

/// Reads the token in clear text: from the bootstrap file if there is one, otherwise from the
/// config file.
///
/// The agent uses this store when the platform has no supported OS secret store yet, or when
/// moving the token there failed. The token is read again each time it is needed, and never
/// kept in memory.
pub struct ConfigStore {
    /// The bootstrap file, read first.
    bootstrap: PathBuf,
    /// Where the settings are read from. It holds no setting, so no token either.
    source: SettingsSource,
}

impl ConfigStore {
    pub fn new(bootstrap: PathBuf, source: SettingsSource) -> Self {
        ConfigStore { bootstrap, source }
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
        let settings = self.source.load().map_err(|err| {
            Error::Internal(format!(
                "Unable to read token from agent configuration: {}",
                reason(&err)
            ))
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

/// Explains why the config could not be read, without using the text of `err`. That text can
/// hold the token: a parse error quotes the line that failed, and a type error quotes the value
/// it found. So the explanation is built only from the kind of error and the setting's name.
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
