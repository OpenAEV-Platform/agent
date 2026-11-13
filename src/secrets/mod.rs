//! Secrets the agent authenticates with (today, only the platform token).
//!
//! The token is read through a [`SecretStore`] on each use instead of being kept in memory.
//! `ConfigStore` is the only store for now; OS secret stores will implement the same trait.

pub(crate) mod config_store;

pub use config_store::ConfigStore;

use crate::common::error_model::Error;
use crate::config::settings::Settings;
use std::sync::Arc;
use zeroize::Zeroizing;

/// Name of the platform token secret.
pub const TOKEN: &str = "token";

pub trait SecretStore: Send + Sync {
    /// Reads the secret. The returned value is wiped from memory when dropped.
    fn get(&self, name: &str) -> Result<Zeroizing<String>, Error>;
}

/// Opens the store the agent reads its token from.
pub fn open() -> Arc<dyn SecretStore> {
    Arc::new(ConfigStore::new(Settings::source()))
}
