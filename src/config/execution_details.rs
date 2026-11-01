use config::ConfigError;
use log::{error, warn};
use serde::Deserialize;
use std::process::{Command, Output, Stdio};

#[cfg(any(target_os = "linux", target_os = "macos"))]
const ROOT_USER: &str = "root";

#[derive(Debug, Deserialize, Clone)]
#[allow(unused)]
pub struct ExecutionDetails {
    pub is_elevated: bool,
    pub is_service: bool,
    pub executed_by_user: String,
}

impl ExecutionDetails {
    // -- COMMAND EXECUTION --

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

    fn whoami(executor: &str, args: &[&str], line_ending: &str) -> (String, String) {
        let output = match Self::invoke_command(executor, "whoami", args) {
            Ok(output) => output,
            Err(spawn_error) => return (String::new(), spawn_error.to_string()),
        };
        let user = Self::decode_output(&output.stdout).replace(line_ending, "");
        let reason = Self::decode_output(&output.stderr).trim().to_string();
        (user, reason)
    }

    // -- USER RESOLUTION --

    fn log_user_fallback(resolved: &str, reason: &str, fallback: &str) {
        if resolved.is_empty() {
            error!("whoami returned no user ({reason}) and neither did {fallback}");
        } else {
            warn!("whoami returned no user ({reason}), falling back to {fallback}: {resolved:?}");
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn resolve_unix_user(whoami_user: &str, uid: &str) -> String {
        if !whoami_user.trim().is_empty() {
            return whoami_user.to_string();
        }
        // uid 0 without a passwd entry is still root, and the elevated/service branch keys on it.
        match uid.trim().parse::<u32>() {
            Ok(0) => String::from(ROOT_USER),
            Ok(parsed_uid) => parsed_uid.to_string(),
            Err(_) => String::new(),
        }
    }

    #[cfg(target_os = "windows")]
    fn resolve_windows_user(whoami_user: &str, env_username: &str) -> String {
        if !whoami_user.trim().is_empty() {
            return whoami_user.to_string();
        }
        env_username.trim().to_string()
    }

    /// `id -u` only runs when `whoami` came back empty, so the common path stays one command.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn get_unix_user(executor: &str, args: &[&str]) -> String {
        let (user, reason) = Self::whoami(executor, args, "\n");
        if !user.trim().is_empty() {
            return user;
        }
        let uid = Self::invoke_command(executor, "id -u", args)
            .map(|output| Self::decode_output(&output.stdout))
            .unwrap_or_default();
        let resolved = Self::resolve_unix_user(&user, &uid);
        Self::log_user_fallback(&resolved, &reason, "id -u");
        resolved
    }

    #[cfg(target_os = "windows")]
    fn get_windows_user(executor: &str, args: &[&str]) -> String {
        let (user, reason) = Self::whoami(executor, args, "\r\n");
        if !user.trim().is_empty() {
            return user;
        }
        let env_username = Self::invoke_command(executor, "$env:USERNAME", args)
            .map(|output| Self::decode_output(&output.stdout))
            .unwrap_or_default();
        let resolved = Self::resolve_windows_user(&user, &env_username);
        Self::log_user_fallback(&resolved, &reason, "$env:USERNAME");
        resolved
    }

    // -- EXECUTION CONTEXT --

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
        let user = Self::get_windows_user(executor, args.as_slice());
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
        if user == ROOT_USER {
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
        if user == ROOT_USER {
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
