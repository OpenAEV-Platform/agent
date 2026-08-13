use config::ConfigError;
use log::error;
use serde::Deserialize;
use std::process::{Command, Output, Stdio};

#[derive(Debug, Deserialize, Clone)]
#[allow(unused)]
pub struct ExecutionDetails {
    pub is_elevated: bool,
    pub is_service: bool,
    pub executed_by_user: String,
}

impl ExecutionDetails {
    pub fn invoke_command(
        executor: &str,
        cmd_expression: &str,
        args: &[&str],
    ) -> std::io::Result<Output> {
        Command::new(executor)
            .args(args)
            .arg(cmd_expression)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?
            .wait_with_output()
    }

    pub fn decode_output(raw_bytes: &[u8]) -> String {
        // Try decoding as UTF-8
        if let Ok(decoded) = String::from_utf8(raw_bytes.to_vec()) {
            return decoded; // Return if successful
        }
        // Fallback to UTF-8 lossy decoding
        String::from_utf8_lossy(raw_bytes).to_string()
    }

    pub fn get_user_from_command(executor: &str, args: &[&str], replace_str: &str) -> String {
        let user_output = Self::invoke_command(executor, "whoami", args);
        let user_result_output = user_output.unwrap().clone();
        let user_err = Self::decode_output(&user_result_output.stderr);
        if !user_err.is_empty() {
            error!(
                "User not returned with whoami command, try to restart the agent : {user_err:?}"
            );
        }
        Self::decode_output(&user_result_output.stdout).replace(replace_str, "")
    }

    /// `whoami` prints nothing when the running uid has no passwd entry (random-uid containers,
    /// systemd DynamicUser), and an empty user breaks job matching server-side.
    pub fn resolve_user(whoami_user: &str, uid: &str) -> String {
        let user = whoami_user.trim();
        if !user.is_empty() {
            return whoami_user.to_string();
        }
        match uid.trim() {
            "" => String::new(),
            "0" => String::from("root"),
            numeric_uid => numeric_uid.to_string(),
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn get_unix_user(executor: &str, args: &[&str]) -> String {
        let user = Self::get_user_from_command(executor, args, "\n");
        if !user.trim().is_empty() {
            return user;
        }
        let uid = Self::invoke_command(executor, "id -u", args)
            .map(|output| Self::decode_output(&output.stdout))
            .unwrap_or_default();
        let resolved = Self::resolve_user(&user, &uid);
        if resolved.is_empty() {
            error!("Could not resolve the executing user from whoami nor from id -u");
        } else {
            log::warn!("whoami returned no user, falling back to uid-derived user {resolved:?}");
        }
        resolved
    }

    #[cfg(target_os = "windows")]
    pub fn new(is_service: bool) -> Result<Self, ConfigError> {
        let executor = "powershell";
        let args = Vec::from([
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-NonInteractive",
            "-NoProfile",
            "-Command",
        ]);
        // No uid to fall back to on Windows, but keep one funnel for the registered user.
        let user = Self::resolve_user(
            &Self::get_user_from_command(executor, args.as_slice(), "\r\n"),
            "",
        );
        let is_elevated_output = Self::invoke_command(executor,
                                                      "([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator);", args.as_slice());
        let is_elevated = Self::decode_output(&is_elevated_output.unwrap().clone().stdout);
        Ok(ExecutionDetails {
            is_elevated: is_elevated.contains("True"),
            is_service,
            executed_by_user: user,
        })
    }

    #[cfg(target_os = "linux")]
    pub fn new(_is_service: bool) -> Result<Self, ConfigError> {
        let executor = "sh";
        let args = vec!["-c"];
        let user = Self::get_unix_user(executor, args.as_slice());
        if user == "root" {
            Ok(ExecutionDetails {
                is_elevated: true,
                is_service: true,
                executed_by_user: user,
            })
        } else {
            let is_elevated_output = Self::invoke_command(executor, "id", args.as_slice());
            let is_elevated = Self::decode_output(&is_elevated_output.unwrap().clone().stdout);
            let is_service_output =
                Self::invoke_command(executor, "systemctl status $PPID", args.as_slice());
            let is_service = Self::decode_output(&is_service_output.unwrap().clone().stdout);
            Ok(ExecutionDetails {
                is_elevated: is_elevated.contains("(sudo)"),
                is_service: is_service
                    .split("\n")
                    .next()
                    .unwrap()
                    .contains("openaev-agent.service"),
                executed_by_user: user,
            })
        }
    }

    #[cfg(target_os = "macos")]
    pub fn new(_is_service: bool) -> Result<Self, ConfigError> {
        let executor = "sh";
        let args = vec!["-c"];
        let user = Self::get_unix_user(executor, args.as_slice());
        if user == "root" {
            Ok(ExecutionDetails {
                is_elevated: true,
                is_service: true,
                executed_by_user: user,
            })
        } else {
            let is_elevated_output = Self::invoke_command(executor, "id", args.as_slice());
            let is_elevated = Self::decode_output(&is_elevated_output.unwrap().clone().stdout);
            let is_service_output = Self::invoke_command(
                executor,
                "launchctl print gui/$(id -u)/io.filigran.openaev-agent-session",
                args.as_slice(),
            );
            let is_service = Self::decode_output(&is_service_output.unwrap().clone().stdout);
            Ok(ExecutionDetails {
                is_elevated: is_elevated.contains("(admin)"),
                is_service: !is_service.contains("openaev-agent-session"),
                executed_by_user: user,
            })
        }
    }
}
