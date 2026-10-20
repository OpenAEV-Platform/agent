//! Where the agent keeps the secrets it uses to authenticate.
//!
//! The installers write the token in clear text in the config file. A bootstrap file next to
//! the executable can also give the agent a token: it wins over the config.
//!
//! In production, if the platform has an OS secret store and the `os_store` feature flag is
//! on, the agent moves the token there.
//! It then reads the token from the store each time it needs it, instead of keeping it in
//! memory.

mod config_store;
mod import;
mod replace;

pub use config_store::ConfigStore;
#[cfg(test)]
pub(crate) use import::cleanup_failure_message;
pub use import::import;
pub(crate) use replace::{recover, remove_backup, replace_file};

use crate::common::error_model::Error;
use crate::config::settings::{Settings, ENV_PRODUCTION};
use log::error;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use zeroize::Zeroizing;

/// Name of the secret the agent uses to authenticate to the platform.
pub const TOKEN: &str = "token";
/// File that gives the agent a token in clear text. The agent deletes it once the token is
/// safe in the store.
pub const BOOTSTRAP_FILE: &str = "openaev-agent-token.bootstrap";
/// Marker file that exists while a token written to the store is not verified yet. It holds
/// no secret. While it exists, the stored token is not trusted.
pub const PENDING_FILE: &str = "openaev-agent-token.pending";

pub trait SecretStore: Send + Sync {
    /// Reads the secret. The returned value is wiped from memory when it is dropped.
    ///
    /// A secret that was never saved is reported with `missing()`. Any other error means the
    /// store could not be read: the secret may exist.
    fn get(&self, name: &str) -> Result<Zeroizing<String>, Error>;
    /// Replaces the secret. The next `get` returns the new value.
    fn set(&self, name: &str, value: &str) -> Result<(), Error>;
}

/// The error a store returns for a secret that was never saved.
pub(crate) fn missing(message: String) -> Error {
    Error::Io(std::io::Error::new(ErrorKind::NotFound, message))
}

/// Whether `err` means the secret was never saved, as opposed to a read failure.
pub(crate) fn is_missing(err: &Error) -> bool {
    matches!(err, Error::Io(err) if err.kind() == ErrorKind::NotFound)
}

/// Opens the store the agent reads its token from.
///
/// `config_token` is the token found in the config file. `os_store` is the feature flag from
/// the `[secrets]` table of the config.
pub fn open(config_token: Option<Zeroizing<String>>, os_store: bool) -> Arc<dyn SecretStore> {
    let install_dir = Settings::install_dir();
    let bootstrap = install_dir.join(BOOTSTRAP_FILE);
    let clear_text: Arc<dyn SecretStore> =
        Arc::new(ConfigStore::new(bootstrap.clone(), Settings::config_file()));
    if Settings::mode() != ENV_PRODUCTION {
        return clear_text;
    }
    let Some(store) = platform_store(&install_dir) else {
        return clear_text;
    };
    choose(
        store,
        clear_text,
        os_store,
        &bootstrap,
        config_token.as_ref().map(|token| token.as_str()),
        &Settings::production_config_path(),
    )
}

/// Picks between the OS store and the clear-text copy. Split from `open` so it can be tested.
///
/// Flag on: the token is moved into the OS store, then read from there.
/// Flag off: nothing is written or deleted. The clear-text token is used, except when the
/// token was already moved while the flag was on: it then has no clear-text copy left, so
/// it is still read from the OS store.
pub(crate) fn choose(
    store: Arc<dyn SecretStore>,
    clear_text: Arc<dyn SecretStore>,
    os_store: bool,
    bootstrap: &Path,
    config_token: Option<&str>,
    config: &Path,
) -> Arc<dyn SecretStore> {
    if os_store {
        return match import(store.as_ref(), bootstrap, config_token, config) {
            Ok(()) => store,
            // We fall back to the clear-text copy: an agent that stays online is better than
            // an agent that stops working.
            Err(err) => {
                error!("Could not move the token to the OS secret store: {err}");
                clear_text
            }
        };
    }
    // The bootstrap file is the way to replace the token, so it wins as with the flag on.
    let no_bootstrap = matches!(read_bootstrap(bootstrap), Ok(None));
    // A token in the store counts only if its write was verified (no pending marker).
    let verified = !bootstrap.with_file_name(PENDING_FILE).exists()
        && store.get(TOKEN).is_ok_and(|token| !token.is_empty());
    if no_bootstrap && verified {
        store
    } else {
        clear_text
    }
}

/// Puts the config back when a crash interrupted its rewrite and left it missing. Call it
/// before loading the settings: they cannot load without the config. Returns the file the
/// config was restored from, if any.
pub fn recover_config() -> Option<PathBuf> {
    (Settings::mode() == ENV_PRODUCTION)
        .then(Settings::production_config_path)
        .and_then(|config| recover(&config))
}

/// The OS secret store of this platform, if the agent supports one here.
fn platform_store(_install_dir: &Path) -> Option<Arc<dyn SecretStore>> {
    None
}

/// Reads the token from the bootstrap file, if there is one.
pub(crate) fn read_bootstrap(path: &Path) -> Result<Option<Zeroizing<String>>, Error> {
    match fs::read_to_string(path) {
        Ok(content) => {
            let content = Zeroizing::new(content);
            let token = content.trim();
            Ok((!token.is_empty()).then(|| Zeroizing::new(token.to_string())))
        }
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(Error::Io(err)),
    }
}
