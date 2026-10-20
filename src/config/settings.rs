use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::env;
use std::fmt;
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

pub const ENV_PRODUCTION: &str = "production";
const ENV_PRODUCTION_CONFIG_FILE: &str = "openaev-agent-config";

#[derive(Deserialize, Clone)]
#[allow(unused)]
pub struct OpenAEV {
    pub url: String,
    // Optional: once the token is moved to the OS secret store, it is no longer in the config.
    // It is wrapped in `Zeroizing` as soon as it is read, so its memory is wiped wherever it
    // is dropped. The config crate makes its own copies while loading the file, and those are
    // not wiped. Once the token is in the OS secret store, those copies no longer exist.
    pub token: Option<Zeroizing<String>>,
    pub unsecured_certificate: bool,
    pub with_proxy: bool,
    pub installation_mode: String,
    pub service_name: String,
    pub tenant_id: String,
}

// Written by hand so that printing the settings never prints the token.
impl fmt::Debug for OpenAEV {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenAEV")
            .field("url", &self.url)
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .field("unsecured_certificate", &self.unsecured_certificate)
            .field("with_proxy", &self.with_proxy)
            .field("installation_mode", &self.installation_mode)
            .field("service_name", &self.service_name)
            .field("tenant_id", &self.tenant_id)
            .finish()
    }
}

fn default_executing_max_time_minutes() -> u64 {
    10
}
fn default_directory_max_time_minutes() -> u64 {
    10
}
fn default_cleanup_interval_seconds() -> u64 {
    180
}
#[derive(Debug, Deserialize, Clone)]
pub struct CleanupSettings {
    #[serde(default = "default_executing_max_time_minutes")]
    pub executing_max_time_minutes: u64,
    #[serde(default = "default_directory_max_time_minutes")]
    pub directory_max_time_minutes: u64,
    #[serde(default = "default_cleanup_interval_seconds")]
    pub cleanup_interval_seconds: u64,
}

impl Default for CleanupSettings {
    fn default() -> Self {
        Self {
            executing_max_time_minutes: default_executing_max_time_minutes(),
            directory_max_time_minutes: default_directory_max_time_minutes(),
            cleanup_interval_seconds: default_cleanup_interval_seconds(),
        }
    }
}

/// The `[secrets]` table of the config. Every key is optional.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct SecretsSettings {
    /// Feature flag: moves the token into the OS secret store (DPAPI on Windows). Off by
    /// default while other systems have no store yet. Turned on with `os_store = true`.
    #[serde(default)]
    pub os_store: bool,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(unused)]
pub struct Settings {
    pub debug: bool,
    pub openaev: OpenAEV,
    #[serde(default)]
    pub cleanup: CleanupSettings,
    #[serde(default)]
    pub secrets: SecretsSettings,
}

impl Settings {
    pub fn mode() -> String {
        env::var("env").unwrap_or_else(|_| ENV_PRODUCTION.into())
    }

    /// The directory of the executable, where the installers put the config and the token.
    pub fn install_dir() -> PathBuf {
        let current_exe_patch = env::current_exe().unwrap();
        current_exe_patch.parent().unwrap().to_path_buf()
    }

    /// The config file the installers write in production.
    pub fn production_config_path() -> PathBuf {
        Self::install_dir().join(format!("{ENV_PRODUCTION_CONFIG_FILE}.toml"))
    }

    pub fn new() -> Result<Self, ConfigError> {
        match Self::config_file() {
            Some(config_file) => Self::from_file(&config_file),
            None => {
                let run_mode = Self::mode();
                Config::builder()
                    .add_source(Environment::with_prefix("openaev"))
                    .add_source(File::with_name("config/default"))
                    .add_source(File::with_name(&format!("config/{run_mode}")).required(false))
                    .build()?
                    .try_deserialize()
            }
        }
    }

    /// The config file the installers write next to the executable, without its extension.
    /// `None` in development, where the settings come from `config/`.
    pub fn config_file() -> Option<PathBuf> {
        (Self::mode() == ENV_PRODUCTION)
            .then(|| Self::install_dir().join(ENV_PRODUCTION_CONFIG_FILE))
    }

    /// Loads the settings the way production does, from `config_file` (without its
    /// extension).
    pub fn from_file(config_file: &Path) -> Result<Self, ConfigError> {
        Config::builder()
            .add_source(Environment::with_prefix("openaev"))
            .add_source(File::with_name(&config_file.display().to_string()).required(true))
            .build()?
            .try_deserialize()
    }
}
