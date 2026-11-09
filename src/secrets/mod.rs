//! Secrets the agent authenticates with (today, only the platform token).
//!
//! The installers write the token in clear text in the config file. In production, on a
//! platform with an OS secret store, the agent moves the token into that store at each start
//! and reads it from there. Otherwise, `ConfigStore` reads it from the config file.
//! Either way, the token is read on each use instead of being kept in memory.

pub(crate) mod config_store;
mod import;

pub use config_store::ConfigStore;
pub use import::import;

use crate::common::error_model::Error;
use crate::config::settings::{Settings, SettingsSource};
use log::error;
use std::sync::Arc;
use zeroize::Zeroizing;

/// Name of the platform token secret.
pub const TOKEN: &str = "token";

pub trait SecretStore: Send + Sync {
    /// Reads the secret. The returned value is wiped from memory when dropped.
    fn get(&self, name: &str) -> Result<Zeroizing<String>, Error>;
    /// Replaces the secret. The next `get` returns the new value.
    fn set(&self, name: &str, value: &str) -> Result<(), Error>;
}

/// Opens the store the agent reads its token from.
///
/// `config_token` is the token the installers wrote in the config file, if it is still there.
pub fn open(config_token: Option<Zeroizing<String>>) -> Arc<dyn SecretStore> {
    let source = Settings::source();
    // The OS store is only used in production, where the installers write the config file.
    if let (Some(store), SettingsSource::File(config)) = (platform_store(), &source) {
        // `SettingsSource` names the file without its extension; the installers write `.toml`.
        let config = config.with_extension("toml");
        if let Err(err) = import(
            store.as_ref(),
            config_token.as_deref().map(String::as_str),
            &config,
        ) {
            error!("{err}");
        }
        return store;
    }
    Arc::new(ConfigStore::new(source))
}

/// The OS secret store of this platform, if the agent supports one here.
fn platform_store() -> Option<Arc<dyn SecretStore>> {
    None
}
