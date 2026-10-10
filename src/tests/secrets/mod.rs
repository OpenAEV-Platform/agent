mod config_store_tests;
mod import_tests;
mod replace_tests;

use crate::common::error_model::Error;
use crate::secrets::SecretStore;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

/// A secret store kept in memory, standing in for the OS one.
#[derive(Default)]
pub struct MemoryStore {
    secrets: Mutex<HashMap<String, String>>,
    pub writes: AtomicUsize,
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

    fn set(&self, name: &str, value: &str) -> Result<(), Error> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.secrets
            .lock()
            .unwrap()
            .insert(name.to_string(), value.to_string());
        Ok(())
    }
}

/// A secret store that cannot be written, like an OS store that is unavailable.
pub struct FailingStore;

impl SecretStore for FailingStore {
    fn get(&self, name: &str) -> Result<Zeroizing<String>, Error> {
        Err(Error::Internal(format!("no secret named {name}")))
    }

    fn set(&self, _name: &str, _value: &str) -> Result<(), Error> {
        Err(Error::Internal("store unavailable".to_string()))
    }
}

/// A fresh, empty directory for one test.
pub fn test_dir(name: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let dir = env::temp_dir().join(format!(
        "openaev-agent-{name}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}
