use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use prost_types::Timestamp;
use std::collections::HashMap;

use crate::types::Operation;

pub fn parse_log_line(line: &str) -> Result<Operation> {
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
        timestamp: LallyStamp::from(timestamp)?,
    })
}

pub struct LallyStamp {}

impl LallyStamp {
    pub fn new() -> Timestamp {
        let now = Utc::now();
        Timestamp {
            seconds: now.timestamp(),
            nanos: now.timestamp_subsec_nanos() as i32,
        }
    }
    pub fn from(rfc3339: &str) -> Result<Timestamp, chrono::ParseError> {
        let datetime = DateTime::parse_from_rfc3339(rfc3339)?;
        let datetime_utc = datetime.with_timezone(&Utc);
        Ok(Timestamp {
            seconds: datetime_utc.timestamp(),
            nanos: datetime_utc.timestamp_subsec_nanos() as i32,
        })
    }
    pub fn to_rfc3339(timestamp: &Timestamp) -> String {
        let naive = Utc
            .timestamp_opt(timestamp.seconds, timestamp.nanos as u32)
            .single()
            .expect("Invalid timestamp");
        naive.to_rfc3339()
    }
}

pub struct KVGetResult {
    pub message: Option<String>,
    pub timestamp: Option<Timestamp>,
}

// impl Ord for Timestamp {
//     fn cmp(&self, other: &Self) -> Ordering {
//         match self.seconds.cmp(&other.seconds) {
//             Ordering::Equal => self.nanos.cmp(&other.nanos),
//             other => other,
//         }
//     }
// }
//
// impl Eq for Timestamp {}
