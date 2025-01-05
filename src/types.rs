use prost_types::Timestamp;

pub struct Operation {
    pub name: String,
    pub level: String,
    pub key: String,
    pub value: Option<String>,
    pub timestamp: Timestamp,
}
