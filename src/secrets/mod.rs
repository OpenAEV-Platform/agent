//! Where the agent keeps the secrets it authenticates with.
//!
//! The agent reads its token through a [`SecretStore`] on each use rather than keeping it in
//! memory. The only store today is the config the installers write; OS secret stores plug in
//! behind the same trait.

pub(crate) mod config_store;

pub use config_store::ConfigStore;

use crate::common::error_model::Error;
use crate::config::settings::Settings;
use std::sync::Arc;
use zeroize::Zeroizing;

/// Name of the secret the agent authenticates to the platform with.
pub const TOKEN: &str = "token";

pub trait SecretStore: Send + Sync {
    /// Reads the secret. The returned value is wiped from memory when dropped.
    fn get(&self, name: &str) -> Result<Zeroizing<String>, Error>;
}

/// Opens the store the agent reads its token from.
pub fn open() -> Arc<dyn SecretStore> {
    Arc::new(ConfigStore::new(Settings::source()))
}
