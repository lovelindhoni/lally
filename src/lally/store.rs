use crate::cluster::services::KvData;
use crate::utils::timestamp::compare_timestamps;
use crate::utils::Operation;
use crate::utils::{parse_aof_log, KVResult};
use anyhow::{Context, Result};
use papaya::HashMap;
use prost_types::Timestamp;
use rapidhash::fast::RandomState;
use std::cmp::Ordering;
use std::path::Path;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::{debug, error, info};

type StoreMap = HashMap<String, (String, Timestamp, bool), RandomState>;

pub struct Store {
    store: StoreMap,
}

impl Store {
    pub async fn new(log_path: &Path) -> Result<Self> {
        info!(
            "Initializing store and replaying AOF log from {:?}",
            log_path
        );

        let store = Store {
            store: HashMap::builder().hasher(RandomState::default()).build(),
        };

        store
            .replay_aof(log_path)
            .await
            .context("Error while replaying AOF log")?;

        Ok(store)
    }

    async fn replay_aof(&self, log_path: &Path) -> Result<()> {
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
        let pin = self.store.pin();

        while let Some(line) = lines.next_line().await? {
            line_count += 1;
            if let Ok(operation) = parse_aof_log(&line) {
                match operation.name.as_str() {
                    "ADD" => {
                        if let Some(value) = operation.value {
                            pin.insert(operation.key.clone(), (value, operation.timestamp, true));
                            debug!("ADD operation: inserted key '{}'", operation.key);
                        } else {
                            error!("Missing value for ADD operation, this shouldn't happen");
                        }
                    }
                    "REMOVE" => {
                        pin.remove(&operation.key);
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
        let pin = self.store.pin();
        let result: Vec<KvData> = pin
            .iter()
            .map(|(key, value)| KvData {
                key: key.clone(),
                value: value.0.clone(),
                timestamp: Some(value.1),
                valid: value.2,
            })
            .collect();
        info!("Store data exported with {} entries", result.len());
        result
    }

    pub fn import_store(&self, store: Vec<KvData>) {
        info!("Importing store data with {} entries", store.len());
        let pin = self.store.pin();
        for data in store {
            if let Some(timestamp) = data.timestamp {
                let new_value = (data.value, timestamp, data.valid);
                pin.update_or_insert_with(
                    data.key.clone(),
                    |existing| {
                        if compare_timestamps(&new_value.1, &existing.1) == Ordering::Greater {
                            new_value.clone()
                        } else {
                            existing.clone()
                        }
                    },
                    || new_value.clone(),
                );
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
            .pin()
            .insert(key.clone(), (value.clone(), timestamp, true));

        KVResult {
            success: true,
            value: None,
            timestamp: Some(timestamp),
        }
    }

    pub fn remove(&self, operation: &Operation) -> KVResult {
        debug!("Performing REMOVE operation for key '{}'", operation.key);
        let pin = self.store.pin();

        // Snapshot the current state. The check-then-update is benign-racy:
        // a concurrent REMOVE arriving between get and update just overwrites
        // with the same tombstone shape.
        match pin.get(&operation.key) {
            Some(existing) if existing.2 => {
                let new_value = (existing.0.clone(), operation.timestamp, false);
                pin.update(operation.key.clone(), |_| new_value.clone());
                debug!("Key '{}' successfully removed", operation.key);
                KVResult {
                    success: true,
                    value: None,
                    timestamp: Some(operation.timestamp),
                }
            }
            Some(_) => {
                error!(
                    "Failed to remove key '{}': it is already marked as invalid",
                    operation.key
                );
                KVResult {
                    success: false,
                    value: None,
                    timestamp: None,
                }
            }
            None => {
                debug!("Key '{}' not found for removal", operation.key);
                KVResult {
                    success: false,
                    value: None,
                    timestamp: None,
                }
            }
        }
    }

    pub fn get(&self, operation: &Operation) -> KVResult {
        debug!("Performing GET operation for key '{}'", operation.key);
        let pin = self.store.pin();

        match pin.get(&operation.key) {
            Some(value) => {
                if value.2 {
                    debug!("Key '{}' found with valid value", operation.key);
                    KVResult {
                        success: true,
                        value: Some(value.0.clone()),
                        timestamp: Some(value.1),
                    }
                } else {
                    debug!("Key '{}' found but marked as invalid", operation.key);
                    KVResult {
                        success: false,
                        value: None,
                        timestamp: None,
                    }
                }
            }
            None => {
                debug!("Key '{}' not found", operation.key);
                KVResult {
                    success: false,
                    value: None,
                    timestamp: None,
                }
            }
        }
    }
}
