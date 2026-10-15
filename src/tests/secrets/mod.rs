mod config_store_tests;

use crate::common::error_model::Error;
use crate::secrets::SecretStore;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

/// A secret store kept in memory, standing in for a real one.
#[derive(Default)]
pub struct MemoryStore {
    secrets: Mutex<HashMap<String, String>>,
}

impl MemoryStore {
    pub fn with_token(token: &str) -> Arc<MemoryStore> {
        let store = MemoryStore::default();
        store
            .secrets
            .lock()
            .unwrap()
            .insert(crate::secrets::TOKEN.to_string(), token.to_string());
        Arc::new(store)
    }
}

impl SecretStore for MemoryStore {
    fn get(&self, name: &str) -> Result<Zeroizing<String>, Error> {
        self.secrets
            .lock()
            .unwrap()
            .get(name)
            .map(|value| Zeroizing::new(value.clone()))
            .ok_or_else(|| Error::Internal(format!("no secret named {name}")))
    }
}

/// A secret store that cannot be read.
pub struct FailingStore;

impl SecretStore for FailingStore {
    fn get(&self, name: &str) -> Result<Zeroizing<String>, Error> {
        Err(Error::Internal(format!("no secret named {name}")))
    }
}
