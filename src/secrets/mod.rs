//! Where the agent keeps the secrets it authenticates with.
//!
//! The installers write the token in clear text in the config file. In production, if the
//! platform has an OS secret store, the agent moves the token there at each start, then reads
//! it from the store. Otherwise, it reads the token from the config file.
//! Either way, the token is read each time it is needed, instead of being kept in memory.

pub(crate) mod config_store;
#[cfg(windows)]
mod dpapi;
mod import;

pub use config_store::ConfigStore;
#[cfg(windows)]
pub use dpapi::DpapiStore;
pub use import::import;

use crate::common::error_model::Error;
use crate::config::settings::{Settings, ENV_PRODUCTION};
use log::error;
use std::sync::Arc;
use zeroize::Zeroizing;

/// Name of the secret the agent authenticates to the platform with.
pub const TOKEN: &str = "token";

pub trait SecretStore: Send + Sync {
    /// Reads the secret. The returned value is wiped from memory when dropped.
    fn get(&self, name: &str) -> Result<Zeroizing<String>, Error>;
    /// Replaces the secret. The next `get` returns the new value.
    fn set(&self, name: &str, value: &str) -> Result<(), Error>;
}

/// Opens the store the agent reads its token from.
///
/// `config_token` is the token found in the config file, if any.
pub fn open(config_token: Option<Zeroizing<String>>) -> Arc<dyn SecretStore> {
    // In development, the token stays in the `config/` folder.
    let os_store = platform_store().filter(|_| Settings::mode() == ENV_PRODUCTION);
    let Some(store) = os_store else {
        return Arc::new(ConfigStore::new(Settings::source()));
    };
    if let Err(err) = import(
        store.as_ref(),
        config_token.as_deref().map(String::as_str),
        &Settings::production_config_path(),
    ) {
        error!("{err}");
    }
    store
}

/// The OS secret store of this platform, if the agent supports one here.
#[cfg(windows)]
fn platform_store() -> Option<Arc<dyn SecretStore>> {
    Some(Arc::new(DpapiStore::new(Settings::install_dir())))
}

#[cfg(not(windows))]
fn platform_store() -> Option<Arc<dyn SecretStore>> {
    None
}
