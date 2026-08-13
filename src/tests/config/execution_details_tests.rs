#[cfg(test)]
mod tests {
    use crate::config::execution_details::ExecutionDetails;

    #[test]
    fn test_resolve_user_keeps_whoami_output_when_present() {
        assert_eq!(ExecutionDetails::resolve_user("root", "0"), "root");
        assert_eq!(ExecutionDetails::resolve_user("alice", "1000"), "alice");
    }

    #[test]
    fn test_resolve_user_keeps_whoami_output_verbatim() {
        // The caller already stripped the platform line ending; do not alter what it kept.
        assert_eq!(
            ExecutionDetails::resolve_user("domain\\service_account", "1000"),
            "domain\\service_account"
        );
    }

    #[test]
    fn test_resolve_user_falls_back_to_numeric_uid_when_whoami_is_empty() {
        assert_eq!(ExecutionDetails::resolve_user("", "63228"), "63228");
        assert_eq!(ExecutionDetails::resolve_user("   ", "63228\n"), "63228");
    }

    #[test]
    fn test_resolve_user_maps_uid_zero_to_root() {
        // A uid 0 with no passwd entry is still root, and the elevated/service branch keys on it.
        assert_eq!(ExecutionDetails::resolve_user("", "0"), "root");
        assert_eq!(ExecutionDetails::resolve_user("", "0\n"), "root");
    }

    #[test]
    fn test_resolve_user_is_empty_when_neither_source_resolves() {
        assert_eq!(ExecutionDetails::resolve_user("", ""), "");
        assert_eq!(ExecutionDetails::resolve_user("\n", "  "), "");
    }
}
