//! Storage of the secrets the agent authenticates with. Today, the only secret is the
//! platform token.
//!
//! The installers write the token in clear text in the config file. In production, when the
//! platform provides an OS secret store, the agent moves the token into that store at each
//! start and reads it from there. Otherwise, `ConfigStore` reads it from the config file.
//! In both cases, the token is read on each use and is not kept in memory.

pub(crate) mod config_store;
mod import;

pub use config_store::ConfigStore;
pub use import::import;

use crate::common::error_model::Error;
use crate::config::settings::{Settings, SettingsSource};
use log::error;
use std::sync::Arc;
use zeroize::Zeroizing;

/// Name of the platform token in the secret stores.
pub const TOKEN: &str = "token";

/// A place where the agent reads its secrets, and writes them when the store supports it.
pub trait SecretStore: Send + Sync {
    /// Returns the secret. The value is wiped from memory when it is dropped.
    fn get(&self, name: &str) -> Result<Zeroizing<String>, Error>;
    /// Stores the secret and replaces any previous value. The next `get` returns it.
    fn set(&self, name: &str, value: &str) -> Result<(), Error>;
}

/// Returns the secret store the agent reads its token from.
///
/// `config_token` is the token found in the config file, if any. In production, when an OS
/// secret store is available, the token is moved into it and then read back once. Any failure
/// stops the agent: the agent cannot authenticate without the token, and the token must not
/// remain in clear text in the config file.
pub fn open(config_token: Option<Zeroizing<String>>) -> Result<Arc<dyn SecretStore>, Error> {
    let source = Settings::source();
    // The OS secret store is used only in production, where the installers write the config
    // file.
    if let (Some(store), SettingsSource::File(config)) = (platform_store(), &source) {
        // The installers write the token in the config file at installation and
        // reinstallation. Only the first start after that finds it; the following starts skip
        // the move.
        if let Some(token) = config_token.as_deref() {
            // `SettingsSource` holds the file name without its extension. The installers write
            // a `.toml` file.
            let config = config.with_extension("toml");
            import(store.as_ref(), token, &config).inspect_err(|err| error!("{err}"))?;
        }
        // Reading the token back confirms that the store can return it. Otherwise, the agent
        // stops now instead of failing on each request.
        store.get(TOKEN).inspect_err(|err| error!("{err}"))?;
        return Ok(store);
    }
    Ok(Arc::new(ConfigStore::new(source)))
}

/// Returns the OS secret store of this platform, or `None` when the agent does not support one.
fn platform_store() -> Option<Arc<dyn SecretStore>> {
    None
}
