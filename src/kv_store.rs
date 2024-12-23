use crate::server::Payload;
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
    pub async fn add(&self, payload: &Payload, wal_needed: bool) -> Result<String> {
        let mut store = self.kv_store.lock().await;
        if wal_needed {
            log(&self.batch, "ADD", "INFO", payload);
        }
        let key = &payload.key;
        let value = payload.value.as_ref().expect("value will be present");
        let add_kv = store.insert(key.clone(), value.clone());
        match add_kv {
            Some(old_value) => {
                if payload.force.unwrap_or(false) {
                    Ok(format!(
                        "kv overwritten in store:- value {} -> {}",
                        old_value,
                        value // payload.value.as_ref().expect("value will be present"),
                    ))
                } else {
                    Err(anyhow!(
                        "key-value already present in store: {}, set force to true to force add it",
                        key
                    ))
                }
            }
            None => Ok(format!(
                "kv added to store {}: {}",
                payload.key,
                payload.value.as_ref().expect("value will be present")
            )),
        }
    }
    pub async fn remove(&self, payload: &Payload, wal_needed: bool) -> Result<String> {
        let mut store = self.kv_store.lock().await;
        if wal_needed {
            log(&self.batch, "REMOVE", "INFO", payload);
        }
        let remove_kv = store.remove(&payload.key);
        match remove_kv {
            Some(removed_key) => Ok(format!("kv removed from store: key {}", removed_key)),
            None => Err(anyhow!(
                "kv not removed from store because it doesn't exists: {}",
                payload.key
            )),
        }
    }
    pub async fn get(&self, payload: &Payload) -> Result<String> {
        let store = self.kv_store.lock().await;
        let get_kv = store.get(&payload.key);
        match get_kv {
            Some(value) => Ok(format!("kv fetched from store: {}", value)),
            None => Err(anyhow!(
                "cannot fetch kv because it doesn't exists: {}",
                payload.key
            )),
        }
    }
    pub async fn update(&self, payload: &Payload, wal_needed: bool) -> Result<String> {
        let mut store = self.kv_store.lock().await;
        if wal_needed {
            log(&self.batch, "UPDATE", "INFO", payload);
        }
        let key = &payload.key;
        let value = payload
            .value
            .as_ref()
            .expect("value will be always present...");
        let update_kv = store.insert(key.clone(), value.clone());
        match update_kv {
            Some(val) => Ok(format!("kv updated in store: {}", val)),
            None => {
                if payload.force.unwrap_or(false) {
                    Ok(format!(
                        "kv added to store because it doesn't exists:- {}: {}",
                        key, value
                    ))
                } else {
                    Err(anyhow!(
                        "key-value doesn't exists:- key: {}, set force to true to force create it",
                        payload.key
                    ))
                }
            }
        }
    }
}
