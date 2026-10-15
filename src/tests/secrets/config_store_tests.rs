#[cfg(test)]
mod tests {
    use crate::secrets::{ConfigStore, SecretStore};

    #[test]
    fn test_config_store_only_holds_the_token() {
        // -- EXECUTE & ASSERT --
        assert!(ConfigStore.get("refresh_token").is_err());
    }
}
