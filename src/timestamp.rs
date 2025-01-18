use chrono::{DateTime, TimeZone, Utc};
use prost_types::Timestamp;
use std::cmp::Ordering;

pub fn create_timestamp() -> Timestamp {
    let now = Utc::now();
    Timestamp {
        seconds: now.timestamp(),
        nanos: now.timestamp_subsec_nanos() as i32,
    }
}
pub fn timestamp_from_rfc3339(rfc3339: &str) -> Result<Timestamp, chrono::ParseError> {
    let datetime = DateTime::parse_from_rfc3339(rfc3339)?;
    let datetime_utc = datetime.with_timezone(&Utc);
    Ok(Timestamp {
        seconds: datetime_utc.timestamp(),
        nanos: datetime_utc.timestamp_subsec_nanos() as i32,
    })
}
pub fn timestamp_to_rfc3339(timestamp: &Timestamp) -> String {
    let naive = Utc
        .timestamp_opt(timestamp.seconds, timestamp.nanos as u32)
        .single()
        .expect("Invalid timestamp");
    naive.to_rfc3339()
}

pub fn compare_timestamps(a: &Timestamp, b: &Timestamp) -> Ordering {
    let a_nanos = a.seconds as i128 * 1_000_000_000 + a.nanos as i128;
    let b_nanos = b.seconds as i128 * 1_000_000_000 + b.nanos as i128;
    a_nanos.cmp(&b_nanos)
}
