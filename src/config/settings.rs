use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::env;
use std::fmt;
use zeroize::Zeroizing;

const ENV_PRODUCTION: &str = "production";
const ENV_PRODUCTION_CONFIG_FILE: &str = "openaev-agent-config";

#[derive(Deserialize, Clone)]
#[allow(unused)]
pub struct OpenAEV {
    pub url: String,
    // Zeroizing from deserialization on, so that every path that drops it wipes it. The
    // copies the config crate makes while loading the file are not.
    pub token: Zeroizing<String>,
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
            .field("token", &"<redacted>")
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
        let run_mode = Self::mode();
        let config = Config::builder().add_source(Environment::with_prefix("openaev"));
        if run_mode == ENV_PRODUCTION {
            // Get the current executable path
            let current_exe_patch = env::current_exe().unwrap();
            let parent_path = current_exe_patch.parent().unwrap();
            // Join the expected config file with the parent
            let config_file = parent_path.join(ENV_PRODUCTION_CONFIG_FILE);
            let config_path = config_file.display();
            config
                .add_source(File::with_name(&config_path.to_string()).required(true))
                .build()?
                .try_deserialize()
        } else {
            config
                .add_source(File::with_name("config/default"))
                .add_source(File::with_name(&format!("config/{run_mode}")).required(false))
                .build()?
                .try_deserialize()
        }
    }
}
