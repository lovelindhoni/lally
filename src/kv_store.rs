use crate::wal::{log, wal_task};
use anyhow::{anyhow, Result};
use crossbeam::queue::SegQueue;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub struct InMemoryKVStore {
    kv_store: Arc<Mutex<HashMap<String, String>>>,
    batch: Arc<SegQueue<String>>,
}

impl InMemoryKVStore {
    pub fn new() -> Self {
        let store = InMemoryKVStore {
            kv_store: Arc::new(Mutex::new(HashMap::new())),
            batch: Arc::new(SegQueue::new()),
        };
        tokio::spawn(wal_task(store.batch.clone()));
        store
    }
    pub async fn add(&self, key: &str, value: &str, wal_needed: bool) -> Result<String> {
        let mut store = self.kv_store.lock().await;
        if wal_needed {
            log(&self.batch, "ADD", "INFO", key, Some(value));
        }
        let add_kv = store.insert(key.to_owned(), value.to_owned());
        match add_kv {
            Some(old_value) => Ok(format!(
                "kv overwritten in store:- value {} -> {}",
                old_value, value,
            )),
            None => Ok(format!("kv added to store {}: {}", key, value)),
        }
    }
    pub async fn remove(&self, key: &str, wal_needed: bool) -> Result<String> {
        let mut store = self.kv_store.lock().await;
        if wal_needed {
            log(&self.batch, "REMOVE", "INFO", key, None);
        }
        let remove_kv = store.remove(key);
        match remove_kv {
            Some(removed_key) => Ok(format!("kv removed from store: key {}", removed_key)),
            None => Err(anyhow!(
                "kv not removed from store because it doesn't exists: {}",
                key
            )),
        }
    }
    pub async fn get(&self, key: &str) -> Result<String> {
        let store = self.kv_store.lock().await;
        let get_kv = store.get(key);
        match get_kv {
            Some(value) => Ok(format!("kv fetched from store: {}", value)),
            None => Err(anyhow!(
                "cannot fetch kv because it doesn't exists: {}",
                key
            )),
        }
    }
    pub async fn update(&self, key: &str, value: &str, wal_needed: bool) -> Result<String> {
        let mut store = self.kv_store.lock().await;
        if wal_needed {
            log(&self.batch, "UPDATE", "INFO", key, Some(value));
        }
        let update_kv = store.insert(key.to_owned(), value.to_owned());
        match update_kv {
            Some(val) => Ok(format!("kv updated in store: {}", val)),
            None => Ok(format!(
                "kv added to store because it doesn't exists:- {}: {}",
                key, value
            )),
        }
    }
}
