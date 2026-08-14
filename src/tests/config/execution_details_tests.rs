#[cfg(test)]
mod tests {
    use crate::config::execution_details::ExecutionDetails;

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn test_resolve_unix_user_keeps_whoami_output_when_present() {
        assert_eq!(ExecutionDetails::resolve_unix_user("root", "0"), "root");
        assert_eq!(ExecutionDetails::resolve_unix_user("alice", "1000"), "alice");
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn test_resolve_unix_user_falls_back_to_numeric_uid_when_whoami_is_empty() {
        assert_eq!(ExecutionDetails::resolve_unix_user("", "63228"), "63228");
        assert_eq!(ExecutionDetails::resolve_unix_user("   ", "63228\n"), "63228");
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn test_resolve_unix_user_maps_uid_zero_to_root() {
        // uid 0 without a passwd entry is still root, and the elevated/service branch keys on it.
        assert_eq!(ExecutionDetails::resolve_unix_user("", "0"), "root");
        assert_eq!(ExecutionDetails::resolve_unix_user("", "0\n"), "root");
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn test_resolve_unix_user_is_empty_when_uid_is_not_numeric() {
        assert_eq!(ExecutionDetails::resolve_unix_user("", ""), "");
        assert_eq!(ExecutionDetails::resolve_unix_user("\n", "  "), "");
        assert_eq!(ExecutionDetails::resolve_unix_user("", "id: no such user"), "");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_resolve_windows_user_keeps_whoami_output_when_present() {
        assert_eq!(
            ExecutionDetails::resolve_windows_user("domain\\service_account", "service_account"),
            "domain\\service_account"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_resolve_windows_user_falls_back_to_env_username() {
        assert_eq!(
            ExecutionDetails::resolve_windows_user("", "service_account\r\n"),
            "service_account"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_resolve_windows_user_is_empty_when_neither_source_resolves() {
        assert_eq!(ExecutionDetails::resolve_windows_user("", ""), "");
        assert_eq!(ExecutionDetails::resolve_windows_user("\r\n", "  "), "");
    }
}
