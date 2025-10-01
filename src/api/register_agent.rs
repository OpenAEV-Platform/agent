use super::Client;
use crate::common::error_model::Error;
use network_interface::NetworkInterface;
use network_interface::NetworkInterfaceConfig;
use serde::Deserialize;
use serde_json::json;
use std::env;
use std::path::PathBuf;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const MAC_ADDRESS_FILTERED_1: &str = "FF:FF:FF:FF:FF:FF";
const MAC_ADDRESS_FILTERED_2: &str = "00:00:00:00:00:00";
const MAC_ADDRESS_FILTERED_3: &str = "01:80:C2:00:00:00";
const IP_ADDRESS_FILTERED_1: &str = "::1";
const IP_ADDRESS_FILTERED_2: &str = "127.";
const IP_ADDRESS_FILTERED_3: &str = "169.254.";

#[derive(Debug, Deserialize)]
pub struct RegisterAgentResponse {
    #[allow(dead_code)]
    pub asset_id: String,
}

pub fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

pub fn get_arch() -> String {
    let arch = match env::consts::ARCH {
        "aarch64" => "arm64", // Mac still use the old nomenclature
        other => other,
    };
    String::from(arch)
}

pub fn get_operating_system() -> String {
    match env::consts::OS {
        "macos" => String::from("MacOS"),
        other => capitalize(other),
    }
}

pub fn get_exe_dir() -> PathBuf {
    let exe_path = env::current_exe().unwrap();
    exe_path.parent().unwrap().to_path_buf()
}

impl Client {
    pub fn register_agent(
        &self,
        is_service: bool,
        is_elevated: bool,
        executed_by_user: String,
        installation_mode: String,
        service_name: String,
    ) -> Result<RegisterAgentResponse, Error> {
        // region Build the content to register
        let networks = NetworkInterface::show().unwrap();
        let mut mac_addresses: Vec<String> = networks
            .iter()
            .map(|interface| &interface.mac_addr)
            .filter_map(|mac| mac.clone())
            .collect();
        let mut ip_addresses: Vec<String> = networks
            .iter()
            .flat_map(|interface| &interface.addr)
            .map(|addr| addr.ip().to_string())
            .collect();
        mac_addresses.retain(|mac| {
            mac != MAC_ADDRESS_FILTERED_1
                && mac != MAC_ADDRESS_FILTERED_2
                && mac != MAC_ADDRESS_FILTERED_3
        });
        ip_addresses.retain(|ip| {
            ip != IP_ADDRESS_FILTERED_1
                && !ip.starts_with(IP_ADDRESS_FILTERED_2)
                && !ip.starts_with(IP_ADDRESS_FILTERED_3)
        });
        let post_data = json!({
          "asset_name": hostname::get()?.to_string_lossy(),
          "asset_external_reference": mid::get("openaev").unwrap(),
          "endpoint_agent_version": VERSION,
          "endpoint_ips": ip_addresses,
          "endpoint_platform": get_operating_system(),
          "endpoint_arch": get_arch(),
          "endpoint_mac_addresses": mac_addresses,
          "endpoint_hostname": hostname::get()?.to_string_lossy(),
          "agent_is_service": is_service,
          "agent_is_elevated": is_elevated,
          "agent_executed_by_user": executed_by_user,
          "agent_installation_mode": installation_mode,
          "agent_installation_directory": get_exe_dir(),
          "agent_service_name": service_name,
        });
        // endregion
        // Post the input to the OpenAEV API
        match self.post("/api/endpoints/register").json(&post_data).send() {
            Ok(response) => {
                if response.status().is_success() {
                    response
                        .json::<RegisterAgentResponse>()
                        .map_err(|e| Error::Internal(e.to_string()))
                } else {
                    let msg = response
                        .text()
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    Err(Error::Api(msg))
                }
            }
            Err(err) => Err(Error::Internal(err.to_string())),
        }
    }
}
