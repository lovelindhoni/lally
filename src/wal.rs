use crate::kv_store::InMemoryKVStore;
use anyhow::Result;
use chrono::prelude::Utc;
use crossbeam::queue::SegQueue;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::time::{interval, Duration};

pub async fn replay(kv_store: &InMemoryKVStore) -> Result<()> {
    let path = "/home/lovelindhoni/dev/projects/kvr/kvrlog.txt";
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)
        .await?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    while let Some(line) = lines.next_line().await? {
        let mut pairs: HashMap<String, String> = HashMap::new();
        for part in line.split_whitespace() {
            if let Some((key, value)) = part.split_once('=') {
                pairs.insert(key.to_string(), value.to_string());
            }
        }
        let operation = pairs
            .get("operation")
            .expect("operation will always be present...");
        let key = pairs.get("key").expect("key will always be present...");
        let unquoted_key = &key[1..key.len() - 1];
        let value = pairs.get("value").expect("value will always be present...");
        let unquoted_value = &value[1..value.len() - 1];
        if operation == "ADD" {
            let _add_op = kv_store.add(unquoted_key, unquoted_value, false).await;
        } else if operation == "REMOVE" {
            let _remove_op = kv_store.remove(unquoted_key, false).await;
        } else if operation == "UPDATE" {
            let _update_op = kv_store.update(unquoted_key, unquoted_value, false).await;
        }
    }
    Ok(())
}

pub fn log(
    batch: &Arc<SegQueue<String>>,
    operation: &str,
    level: &str,
    key: &str,
    value: Option<&str>,
) {
    let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
    let operation_log = format!(
        "time={} operation={} level={} key=\"{}\" value=\"{}\"",
        timestamp,
        operation,
        level,
        key,
        value.unwrap_or("None")
    );
    batch.push(operation_log);
}

pub async fn wal_task(batch: Arc<SegQueue<String>>) {
    let path = "/home/lovelindhoni/dev/projects/kvr/kvrlog.txt";
    let file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .await
        .unwrap();
    let mut batch_writer = BufWriter::new(file);
    let mut interval = interval(Duration::from_millis(100));
    loop {
        interval.tick().await;
        let batch_size = batch.len();
        if batch_size > 0 {
            for _ in 0..batch_size {
                if let Some(log) = batch.pop() {
                    batch_writer.write_all(log.as_bytes()).await.unwrap();
                    batch_writer.write_all(b"\n").await.unwrap();
                }
            }
            batch_writer.flush().await.unwrap();
        }
    }
}
