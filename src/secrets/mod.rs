//! Where the agent keeps the secrets it authenticates with.
//!
//! The installers hand the token over in clear text, in a bootstrap file next to the
//! executable (older installers wrote it in the config). In production the agent moves it
//! into the OS secret store when the platform has one, and reads it back from there on
//! each use rather than keeping it in memory.

mod config_store;
mod import;
mod replace;

pub use config_store::ConfigStore;
pub use import::import;
pub(crate) use replace::replace_file;

use crate::common::error_model::Error;
use crate::config::settings::{Settings, ENV_PRODUCTION};
use log::error;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::sync::Arc;
use zeroize::Zeroizing;

/// Name of the secret the agent authenticates to the platform with.
pub const TOKEN: &str = "token";
/// Clear-text token written by the installers, removed once moved to the store.
pub const BOOTSTRAP_FILE: &str = "openaev-agent-token.bootstrap";

pub trait SecretStore: Send + Sync {
    /// Reads the secret. The returned value is wiped from memory when dropped.
    fn get(&self, name: &str) -> Result<Zeroizing<String>, Error>;
    /// Replaces the secret; the next `get` returns the new value.
    fn set(&self, name: &str, value: &str) -> Result<(), Error>;
}

/// Opens the store the agent reads its token from.
///
/// `config_token` is the token found in the config, if an older installer left it there.
pub fn open(config_token: Option<Zeroizing<String>>) -> Arc<dyn SecretStore> {
    let install_dir = Settings::install_dir();
    let bootstrap = install_dir.join(BOOTSTRAP_FILE);
    if Settings::mode() == ENV_PRODUCTION {
        if let Some(store) = platform_store(&install_dir) {
            let config_token = config_token.as_ref().map(|token| token.as_str());
            match import(
                store.as_ref(),
                &bootstrap,
                config_token,
                &Settings::production_config_path(),
            ) {
                Ok(()) => return store,
                // Staying online on the clear-text copy beats losing the agent.
                Err(err) => error!("Could not move the token to the OS secret store: {err}"),
            }
        }
    }
    Arc::new(ConfigStore::new(bootstrap))
}

/// The OS secret store of this platform, if the agent supports one there.
fn platform_store(_install_dir: &Path) -> Option<Arc<dyn SecretStore>> {
    None
}

/// Reads the token the installer left in the bootstrap file, if there is one.
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
