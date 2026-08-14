use config::ConfigError;
use log::{error, warn};
use serde::Deserialize;
use std::process::{Command, Output, Stdio};

#[cfg(any(target_os = "linux", target_os = "macos"))]
const ROOT_USER: &str = "root";

#[cfg(target_os = "windows")]
const IS_ADMIN_EXPRESSION: &str = concat!(
    "([Security.Principal.WindowsPrincipal] ",
    "[Security.Principal.WindowsIdentity]::GetCurrent())",
    ".IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator);"
);

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

    /// Empty rather than a panic when the command cannot even be spawned: every caller here reads
    /// a probe whose absence means "not elevated" / "not a service", never a fatal condition.
    fn command_stdout(executor: &str, cmd_expression: &str, args: &[&str]) -> String {
        Self::invoke_command(executor, cmd_expression, args)
            .map(|output| Self::decode_output(&output.stdout))
            .unwrap_or_default()
    }

    // -- USER RESOLUTION --

    /// Returns the `whoami` output and the reason it came back empty, so the caller reports the
    /// failure once alongside the fallback it picked instead of logging on its behalf.
    fn whoami(executor: &str, args: &[&str], line_ending: &str) -> (String, String) {
        match Self::invoke_command(executor, "whoami", args) {
            Ok(output) => (
                Self::decode_output(&output.stdout).replace(line_ending, ""),
                Self::decode_output(&output.stderr).trim().to_string(),
            ),
            Err(spawn_error) => (String::new(), spawn_error.to_string()),
        }
    }

    /// One line per failed `whoami`, naming the reason and what the fallback produced. The former
    /// "try to restart the agent" error was both misleading and doubled by the fallback warning.
    fn log_user_fallback(resolved: &str, reason: &str, fallback: &str) {
        if resolved.is_empty() {
            error!("whoami returned no user ({reason}) and neither did {fallback}");
        } else {
            warn!("whoami returned no user ({reason}), falling back to {fallback}: {resolved:?}");
        }
    }

    /// `whoami` prints nothing when the running uid has no passwd entry (random-uid containers,
    /// systemd DynamicUser), and an empty user breaks job matching server-side.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(crate) fn resolve_unix_user(whoami_user: &str, uid: &str) -> String {
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

    /// Windows has no uid to fall back to, but `$env:USERNAME` survives a `whoami` that a hardened
    /// PowerShell policy or a broken account lookup refused. It carries no domain prefix.
    #[cfg(target_os = "windows")]
    pub(crate) fn resolve_windows_user(whoami_user: &str, env_username: &str) -> String {
        if !whoami_user.trim().is_empty() {
            return whoami_user.to_string();
        }
        env_username.trim().to_string()
    }

    /// `id -u` only runs when `whoami` came back empty, so the common path stays one command.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn get_unix_user(executor: &str, args: &[&str]) -> String {
        let (whoami_user, reason) = Self::whoami(executor, args, "\n");
        if !whoami_user.trim().is_empty() {
            return whoami_user;
        }
        let uid = Self::command_stdout(executor, "id -u", args);
        let resolved = Self::resolve_unix_user(&whoami_user, &uid);
        Self::log_user_fallback(&resolved, &reason, "id -u");
        resolved
    }

    #[cfg(target_os = "windows")]
    fn get_windows_user(executor: &str, args: &[&str]) -> String {
        let (whoami_user, reason) = Self::whoami(executor, args, "\r\n");
        if !whoami_user.trim().is_empty() {
            return whoami_user;
        }
        let env_username = Self::command_stdout(executor, "$env:USERNAME", args);
        let resolved = Self::resolve_windows_user(&whoami_user, &env_username);
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
        let is_elevated = Self::command_stdout(executor, IS_ADMIN_EXPRESSION, args.as_slice());
        Ok(ExecutionDetails {
            is_elevated: is_elevated.contains("True"),
            is_service,
            executed_by_user: user,
        })
    }

    /// Shared by Linux and macOS: only the elevated and service probes differ between them.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub fn new(_is_service: bool) -> Result<Self, ConfigError> {
        let executor = "sh";
        let args = vec!["-c"];
        let user = Self::get_unix_user(executor, args.as_slice());
        if user == ROOT_USER {
            return Ok(ExecutionDetails {
                is_elevated: true,
                is_service: true,
                executed_by_user: user,
            });
        }
        Ok(ExecutionDetails {
            is_elevated: Self::is_elevated_unix(executor, args.as_slice()),
            is_service: Self::is_service_unix(executor, args.as_slice()),
            executed_by_user: user,
        })
    }

    // -- PLATFORM PROBES --

    #[cfg(target_os = "linux")]
    fn is_elevated_unix(executor: &str, args: &[&str]) -> bool {
        Self::command_stdout(executor, "id", args).contains("(sudo)")
    }

    #[cfg(target_os = "macos")]
    fn is_elevated_unix(executor: &str, args: &[&str]) -> bool {
        Self::command_stdout(executor, "id", args).contains("(admin)")
    }

    #[cfg(target_os = "linux")]
    fn is_service_unix(executor: &str, args: &[&str]) -> bool {
        Self::command_stdout(executor, "systemctl status $PPID", args)
            .lines()
            .next()
            .unwrap_or_default()
            .contains("openaev-agent.service")
    }

    #[cfg(target_os = "macos")]
    fn is_service_unix(executor: &str, args: &[&str]) -> bool {
        !Self::command_stdout(
            executor,
            "launchctl print gui/$(id -u)/io.filigran.openaev-agent-session",
            args,
        )
        .contains("openaev-agent-session")
    }
}
