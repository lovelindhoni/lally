use crate::cluster::services::KvData;
use crate::utils::timestamp::compare_timestamps;
use crate::utils::Operation;
use crate::utils::{parse_aof_log, KVResult};
use anyhow::{Context, Result};
use dashmap::DashMap;
use prost_types::Timestamp;
use std::cmp::Ordering;
use std::path::Path;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::{debug, error, info};

pub struct Store {
    store: DashMap<String, (String, Timestamp, bool)>,
}

impl Store {
    pub async fn new(log_path: &Path) -> Result<Self> {
        // Log when the store is being created
        info!(
            "Initializing store and replaying AOF log from {:?}",
            log_path
        );

        let store = Store {
            store: DashMap::new(),
        };

        // Replay the AOF file and log the result
        store
            .replay_aof(log_path)
            .await
            .context("Error while replaying AOF log")?;

        Ok(store)
    }

    async fn replay_aof(&self, log_path: &Path) -> Result<()> {
        // Log the start of the AOF replay
        info!("Starting AOF replay from {:?}", log_path);

        let file = BufReader::new(
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(log_path)
                .await
                .context("Failed to open log file")?,
        );

        let mut lines = file.lines();
        let mut line_count = 0;

        while let Some(line) = lines.next_line().await? {
            line_count += 1;
            if let Ok(operation) = parse_aof_log(&line) {
                match operation.name.as_str() {
                    "ADD" => {
                        if let Some(value) = operation.value {
                            self.store
                                .insert(operation.key.clone(), (value, operation.timestamp, true));
                            debug!("ADD operation: inserted key '{}'", operation.key);
                        } else {
                            error!("Missing value for ADD operation, this shouldn't happen");
                        }
                    }
                    "REMOVE" => {
                        self.store.remove(&operation.key);
                        debug!("REMOVE operation: removed key '{}'", operation.key);
                    }
                    _ => error!("Unknown operation: {}", operation.name),
                }
            } else if !line.trim().is_empty() {
                error!("Failed to parse log line {}: {}", line_count, line);
            }
        }

        info!("AOF replay completed with {} lines processed", line_count);
        Ok(())
    }

    pub fn export_store(&self) -> Vec<KvData> {
        info!("Exporting store data");
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
        info!("Store data exported with {} entries", result.len());
        result
    }

    pub fn import_store(&self, store: Vec<KvData>) {
        info!("Importing store data with {} entries", store.len());
        for data in store {
            if let Some(timestamp) = data.timestamp {
                // timestamp would be always present
                let new_value = (data.value, timestamp, data.valid);
                self.store
                    .entry(data.key.clone())
                    .and_modify(|existing_value| {
                        if compare_timestamps(&new_value.1, &existing_value.1) == Ordering::Greater
                        {
                            *existing_value = new_value.clone();
                            debug!("Updated key '{}'", data.key);
                        }
                    })
                    .or_insert(new_value);

                debug!("Imported key '{}'", data.key);
            }
        }
        info!("Store import completed");
    }

    pub fn add(&self, operation: &Operation) -> KVResult {
        let key = &operation.key;
        let timestamp = operation.timestamp;
        let value = operation.value.as_ref().expect("value will be present");

        debug!(
            "Performing ADD operation for key '{}' with value '{}'",
            key, value
        );
        self.store
            .insert(key.clone(), (value.clone(), timestamp, true));

        KVResult {
            success: true,
            value: None,
            timestamp: Some(timestamp),
        }
    }

    pub fn remove(&self, operation: &Operation) -> KVResult {
        let remove_kv = self.store.get_mut(&operation.key);

        debug!("Performing REMOVE operation for key '{}'", operation.key);

        if let Some(mut value) = remove_kv {
            if !value.2 {
                error!(
                    "Failed to remove key '{}': it is already marked as invalid",
                    operation.key
                );
                return KVResult {
                    success: false,
                    value: None,
                    timestamp: None,
                };
            } else {
                value.1 = operation.timestamp;
                value.2 = false;
                info!("Key '{}' successfully removed", operation.key);
                return KVResult {
                    success: true,
                    value: None,
                    timestamp: Some(value.1),
                };
            }
        }

        info!("Key '{}' not found for removal", operation.key);
        KVResult {
            success: false,
            value: None,
            timestamp: None,
        }
    }

    pub fn get(&self, operation: &Operation) -> KVResult {
        let get_kv = self.store.get(&operation.key);

        debug!("Performing GET operation for key '{}'", operation.key);

        match get_kv {
            Some(value) => {
                if value.2 {
                    // if the kv is found and marked as valid alone
                    info!("Key '{}' found with valid value", operation.key);
                    KVResult {
                        success: true,
                        value: Some(value.0.clone()),
                        timestamp: Some(value.1),
                    }
                } else {
                    info!("Key '{}' found but marked as invalid", operation.key);
                    KVResult {
                        success: false,
                        value: None,
                        timestamp: None,
                    }
                }
            }
            None => {
                info!("Key '{}' not found", operation.key);
                KVResult {
                    success: false,
                    value: None,
                    timestamp: None,
                }
            }
        }
    }
}
