use crate::types::Operation;
use crate::utils::parse_log_line;
use anyhow::{anyhow, Context, Result};
use dashmap::DashMap;
use tokio::fs::{create_dir_all, OpenOptions};
use tokio::io::{AsyncBufReadExt, BufReader};

pub struct Store {
    store: DashMap<String, String>,
}

impl Store {
    pub async fn new() -> Result<Self> {
        let store = Store {
            store: DashMap::new(),
        };
        store.replay_logs().await?;
        Ok(store)
    }
    async fn replay_logs(&self) -> Result<()> {
        // the path would be passed from a config
        let path = String::from("/home/lovelindhoni/dev/projects/lally/lallylog.txt");
        create_dir_all("/home/lovelindhoni/dev/projects/lally")
            .await
            .context("Failed to create WAL directory")?;

        let file = BufReader::new(
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(path)
                .await
                .context("Failed to open log file")
                .unwrap(),
        );

        let mut lines = file.lines();

        while let Some(line) = lines.next_line().await? {
            if let Ok(operation_data) = parse_log_line(&line) {
                match operation_data.name.as_str() {
                    "ADD" => {
                        if let Some(value) = operation_data.value {
                            self.store.insert(operation_data.key, value);
                        } else {
                            eprintln!(
                                "Missing value for ADD operation, this shouldn't be happening"
                            );
                        }
                    }
                    "REMOVE" => {
                        self.store.remove(&operation_data.key);
                    }
                    _ => eprintln!("Unknown operation: {}", operation_data.name),
                }
            } else if !line.trim().is_empty() {
                eprintln!("Failed to parse log: {}", line);
            }
        }
        Ok(())
    }

    pub async fn add(&self, operation: Operation) -> Result<String> {
        let key = operation.key;
        let value = operation.value.as_ref().expect("value will be present");
        let add_kv = self.store.insert(key.clone(), value.clone());
        match add_kv {
            Some(old_value) => Ok(format!(
                "kv updated in store:- value {} -> {}",
                old_value, value
            )),
            None => Ok(format!("kv added to store {}: {}", key, value)),
        }
    }
    pub async fn remove(&self, operation: Operation) -> Result<String> {
        let remove_kv = self.store.remove(&operation.key);
        match remove_kv {
            Some(removed_key) => Ok(format!("kv removed from store: key {}", removed_key.1)),
            None => Err(anyhow!(
                "kv not removed from store because it doesn't exists: {}",
                operation.key
            )),
        }
    }

    pub async fn get(&self, operation: Operation) -> Result<String> {
        let get_kv = self.store.get(&operation.key);
        match get_kv {
            Some(value) => Ok(format!("kv fetched from store: {}", value.value())),
            None => Err(anyhow!(
                "cannot fetch kv because it doesn't exists: {}",
                operation.key
            )),
        }
    }
}
