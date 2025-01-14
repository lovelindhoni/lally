use std::path::Path;

use crate::cluster::services::KvData;
use crate::utils::Operation;
use crate::utils::{compare_timestamps, parse_log_line, KVGetResult};
use anyhow::{anyhow, Context, Result};
use dashmap::DashMap;
use prost_types::Timestamp;
use std::cmp::Ordering;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncBufReadExt, BufReader};

pub struct Store {
    store: DashMap<String, (String, Timestamp, bool)>,
}

impl Store {
    pub async fn new(log_path: &Path) -> Result<Self> {
        let store = Store {
            store: DashMap::new(),
        };
        store.replay_logs(log_path).await?;
        Ok(store)
    }
    async fn replay_logs(&self, log_path: &Path) -> Result<()> {
        let file = BufReader::new(
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(log_path)
                .await
                .context("Failed to open log file")
                .unwrap(),
        );

        let mut lines = file.lines();

        while let Some(line) = lines.next_line().await? {
            if let Ok(operation) = parse_log_line(&line) {
                match operation.name.as_str() {
                    "ADD" => {
                        if let Some(value) = operation.value {
                            self.store
                                .insert(operation.key, (value, operation.timestamp, true));
                        } else {
                            eprintln!(
                                "Missing value for ADD operation, this shouldn't be happening"
                            );
                        }
                    }
                    "REMOVE" => {
                        self.store.remove(&operation.key);
                    }
                    _ => eprintln!("Unknown operation: {}", operation.name),
                }
            } else if !line.trim().is_empty() {
                eprintln!("Failed to parse log: {}", line);
            }
        }
        Ok(())
    }

    pub fn export_store(&self) -> Vec<KvData> {
        let mut result = Vec::new();
        for entry in self.store.iter() {
            let value = entry.value().clone();
            let temp = KvData {
                key: entry.key().clone(),
                value: value.0,
                timestamp: Some(value.1),
                valid: value.2,
            };
            result.push(temp);
        }
        result
    }

    pub fn import_store(&self, store: Vec<KvData>) {
        for data in store {
            let new_value = (data.value, data.timestamp.unwrap(), data.valid);
            self.store
                .entry(data.key)
                .and_modify(|existing_value| {
                    if compare_timestamps(&new_value.1, &existing_value.1) == Ordering::Greater {
                        *existing_value = new_value.clone();
                    }
                })
                .or_insert(new_value);
        }
    }

    pub fn add(&self, operation: &Operation) -> Result<String> {
        let key = &operation.key;
        let timestamp = operation.timestamp;
        let value = operation.value.as_ref().expect("value will be present");
        let add_kv = self
            .store
            .insert(key.clone(), (value.clone(), timestamp, true));
        match add_kv {
            Some(old_value) => Ok(format!(
                "kv added to store:- value {} -> {}",
                old_value.0, value
            )),
            None => Ok(format!("kv added to store {}: {}", key.clone(), value)),
        }
    }
    pub fn remove(&self, operation: &Operation) -> Result<String> {
        let remove_kv = self.store.get_mut(&operation.key);
        if let Some(mut value) = remove_kv {
            if !value.2 {
                Err(anyhow!(
                    "kv not removed from store because it doesn't exists: {}",
                    operation.key
                ))
            } else {
                value.1 = operation.timestamp;
                value.2 = false;
                Ok(format!("kv removed from store: key {}", value.0))
            }
        } else {
            Err(anyhow!(
                "kv not removed from store because it doesn't exists: {}",
                operation.key
            ))
        }
    }

    pub fn get(&self, operation: &Operation) -> KVGetResult {
        let get_kv = self.store.get(&operation.key);
        match get_kv {
            Some(value) => {
                if !value.2 {
                    KVGetResult {
                        message: None,
                        timestamp: Some(value.1),
                    }
                } else {
                    KVGetResult {
                        message: Some(value.0.clone()),
                        timestamp: Some(value.1),
                    }
                }
            }
            None => KVGetResult {
                message: None,
                timestamp: None,
            },
        }
    }
}
