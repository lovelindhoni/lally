use crate::timestamp::timestamp_from_rfc3339;
use anyhow::{Context, Result};
use prost_types::Timestamp;
use std::collections::HashMap;

pub fn parse_aof_log(line: &str) -> Result<Operation> {
    if line.trim().is_empty() {
        return Err(anyhow::anyhow!("Empty line"));
    }

    let pairs: HashMap<String, String> = line
        .split_whitespace()
        .filter_map(|part| part.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    let operation = pairs
        .get("operation")
        .context("Missing operation field")?
        .to_string();
    let key = pairs
        .get("key")
        .context("Missing key field")?
        .trim_matches('"')
        .to_string();
    let value = pairs.get("value").map(|v| v.trim_matches('"').to_string());
    let level = pairs
        .get("level")
        .context("Missing level field")?
        .trim_matches('"')
        .to_string();
    let timestamp = pairs
        .get("timestamp")
        .context("Missing timestamp field")?
        .trim_matches('"');

    Ok(Operation {
        name: operation,
        key,
        value,
        level,
        timestamp: timestamp_from_rfc3339(timestamp)
            .context("failed to parse rfc3339 to Timestamp")?,
    })
}

#[derive(Debug)]
pub struct Operation {
    pub name: String,
    pub level: String,
    pub key: String,
    pub value: Option<String>,
    pub timestamp: Timestamp,
}

pub struct KVResult {
    pub success: bool,
    pub value: Option<String>, // Used for `get` operation
    pub timestamp: Option<Timestamp>,
}
