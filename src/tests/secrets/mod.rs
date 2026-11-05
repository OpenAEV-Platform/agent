mod config_store_tests;

use crate::common::error_model::Error;
use crate::secrets::SecretStore;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
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
        store.set_token(token);
        Arc::new(store)
    }

    /// Replaces the token, like a rotation or a store that can be read again.
    pub fn set_token(&self, token: &str) {
        self.secrets
            .lock()
            .unwrap()
            .insert(crate::secrets::TOKEN.to_string(), token.to_string());
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

/// A directory for one test, deleted with everything in it when dropped.
pub struct TestDir(PathBuf);

impl Deref for TestDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A fresh, empty directory for one test, so that tests running in parallel never share a
/// file. Keep it in a variable for the whole test: it is deleted when dropped.
pub fn test_dir(name: &str) -> TestDir {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let dir = env::temp_dir().join(format!(
        "openaev-agent-{name}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    TestDir(dir)
}
