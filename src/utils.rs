use anyhow::{Context, Result};
use std::collections::HashMap;

use crate::types::Operation;

pub fn parse_log_line(line: &str) -> Result<Operation> {
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

    Ok(Operation {
        name: operation,
        key,
        value,
        level,
    })
}
