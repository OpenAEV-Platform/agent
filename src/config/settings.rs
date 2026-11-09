use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::env;
use std::fmt;
use std::path::PathBuf;
use zeroize::Zeroizing;

const ENV_PRODUCTION: &str = "production";
const ENV_PRODUCTION_CONFIG_FILE: &str = "openaev-agent-config";

#[derive(Deserialize, Clone)]
#[allow(unused)]
pub struct OpenAEV {
    pub url: String,
    // `None` once the token has been moved to the OS secret store. The value is wiped from
    // memory when dropped. Limitation: the copies made by the `config` crate while parsing the
    // file are not wiped.
    pub token: Option<Zeroizing<String>>,
    pub unsecured_certificate: bool,
    pub with_proxy: bool,
    pub installation_mode: String,
    pub service_name: String,
    pub tenant_id: String,
}

// Written by hand so that the token is never printed.
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

#[derive(Debug, Deserialize, Clone)]
#[allow(unused)]
pub struct Settings {
    pub debug: bool,
    pub openaev: OpenAEV,
    #[serde(default)]
    pub cleanup: CleanupSettings,
}

impl Settings {
    pub fn mode() -> String {
        env::var("env").unwrap_or_else(|_| ENV_PRODUCTION.into())
    }

    pub fn new() -> Result<Self, ConfigError> {
        Self::source().load()
    }

    /// Where to read the settings: the config file next to the executable in production, the
    /// `config/` folder otherwise.
    pub fn source() -> SettingsSource {
        if Self::mode() != ENV_PRODUCTION {
            return SettingsSource::Development;
        }
        let current_exe_path = env::current_exe().unwrap();
        let parent_path = current_exe_path.parent().unwrap();
        SettingsSource::File(parent_path.join(ENV_PRODUCTION_CONFIG_FILE))
    }
}

/// Where the settings are read from. It stores the location only, not the values, so keeping
/// it does not keep the token in memory.
#[derive(Clone, Debug)]
pub enum SettingsSource {
    /// `config/default.toml`, then `config/<env>.toml` if it exists.
    Development,
    /// The config file the installers write, given without its extension.
    File(PathBuf),
}

impl SettingsSource {
    /// Reads the settings from this source. `OPENAEV_*` environment variables override the file.
    pub fn load(&self) -> Result<Settings, ConfigError> {
        let config = Config::builder().add_source(Environment::with_prefix("openaev"));
        let config = match self {
            SettingsSource::Development => config
                .add_source(File::with_name("config/default"))
                .add_source(
                    File::with_name(&format!("config/{}", Settings::mode())).required(false),
                ),
            SettingsSource::File(config_file) => config
                .add_source(File::with_name(&config_file.display().to_string()).required(true)),
        };
        config.build()?.try_deserialize()
    }
}
