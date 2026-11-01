#[cfg(test)]
mod tests {
    use crate::config::execution_details::ExecutionDetails;

    /// Runs a command outside of `ExecutionDetails` so the expected value does not come
    /// from the code under test.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn run(program: &str, args: &[&str]) -> String {
        match std::process::Command::new(program).args(args).output() {
            Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
            Err(_) => String::new(),
        }
    }

    /// uid 0 without a passwd entry is still root, and the elevated/service branch keys on it.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn numeric_uid_fallback(uid: &str) -> String {
        if uid == "0" {
            return String::from("root");
        }
        String::from(uid)
    }

    /// linux and macos resolve the user the same way, only the elevated/service probes differ.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn assert_resolved_user_is_the_current_unix_user() {
        let user = ExecutionDetails::new(false).unwrap().executed_by_user;
        assert!(!user.is_empty(), "the agent must always resolve a user");

        let passwd_name = run("id", &["-un"]);
        let uid_fallback = numeric_uid_fallback(&run("id", &["-u"]));

        assert!(
            user == passwd_name || user == uid_fallback,
            "resolved user {user:?} must be the passwd name {passwd_name:?} \
             or the numeric uid fallback {uid_fallback:?}"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_executed_by_user_is_the_current_linux_user() {
        assert_resolved_user_is_the_current_unix_user();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_executed_by_user_is_the_current_macos_user() {
        assert_resolved_user_is_the_current_unix_user();
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_executed_by_user_is_the_current_windows_user() {
        let user = ExecutionDetails::new(false).unwrap().executed_by_user;
        assert!(!user.is_empty(), "the agent must always resolve a user");

        // whoami returns `domain\username` while USERNAME holds the bare account name,
        // so only the account part can be compared.
        let account = std::env::var("USERNAME").unwrap_or_default();
        if account.is_empty() {
            return;
        }
        assert!(
            user.to_lowercase().ends_with(&account.to_lowercase()),
            "resolved user {user:?} must end with the current account {account:?}"
        );
    }
}
