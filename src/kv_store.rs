use anyhow::{anyhow, Result};
use chrono::prelude::Utc;
use crossbeam::queue::SegQueue;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};

#[derive(Clone, Debug)]
pub struct InMemoryKVStore {
    kv_store: Arc<Mutex<HashMap<String, String>>>,
    batch: Arc<SegQueue<String>>,
}

impl InMemoryKVStore {
    pub fn new() -> Self {
        let kv = InMemoryKVStore {
            kv_store: Arc::new(Mutex::new(HashMap::new())),
            batch: Arc::new(SegQueue::new()),
        };
        let batch = kv.batch.clone();
        tokio::spawn(async move {
            let path = "/home/lovelindhoni/dev/projects/kvr/kvrlog.txt";
            let mut file = OpenOptions::new()
                .append(true)
                .create(true)
                .open(path)
                .await
                .unwrap();
            let mut interval = interval(Duration::from_millis(100));
            loop {
                interval.tick().await;
                let batch_size = batch.len();
                if batch_size > 0 {
                    let mut batched_logs = String::new();
                    for _ in 0..batch_size {
                        if let Some(log) = batch.pop() {
                            batched_logs.push_str(&log);
                            batched_logs.push_str("\n");
                        }
                    }
                    file.write_all(batched_logs.as_bytes()).await.unwrap();
                    file.flush().await.unwrap();
                }
            }
        });
        kv
    }
    fn wal(&self, operation: &str, level: &str, key: &str, value: Option<&str>) {
        let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let log = format!(
            "time={} operation={} level={} key={} value={}",
            timestamp,
            operation,
            level,
            key,
            value.unwrap_or("None")
        );
        self.batch.push(log);
    }
    pub async fn add(&self, key: &str, value: &str) -> Result<String> {
        let mut store = self.kv_store.lock().await;
        self.wal("ADD", "INFO", key, Some(value));
        let add_kv = store.insert(key.to_owned(), value.to_owned());
        match add_kv {
            Some(old_value) => Ok(format!(
                "kv overwritten in store:- value {} -> {}",
                old_value, value,
            )),
            None => Ok(format!("kv added to store {}: {}", key, value)),
        }
    }
    pub async fn remove(&self, key: &str) -> Result<String> {
        let mut store = self.kv_store.lock().await;
        self.wal("REMOVE", "INFO", key, None);
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
    pub async fn update(&self, key: &str, value: &str) -> Result<String> {
        let mut store = self.kv_store.lock().await;
        self.wal("UPDATE", "INFO", key, Some(value));
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
