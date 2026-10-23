use super::{SecretStore, TOKEN};
use crate::common::error_model::Error;
use crate::config::settings::SettingsSource;
use config::ConfigError;
use std::io;
use zeroize::Zeroizing;

/// The token in the config, where the installers write it. Read again on each use, never
/// kept in memory.
pub struct ConfigStore {
    /// Where the settings are read from. It holds no setting, so no token either.
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
            Error::Internal(format!(
                "Unable to read token from agent configuration: {}",
                reason(&err)
            ))
        })?;
        Ok(settings.openaev.token)
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
