use crossbeam::queue::SegQueue;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::time::{interval, Duration};
use tracing::{debug, error, info};

use super::Hook;
use crate::config::Config;
use crate::utils::timestamp::timestamp_to_rfc3339;
use crate::utils::Operation;

pub struct AppendOnlyLog {
    buffer: SegQueue<String>,
    flush_interval: Duration,
    path: PathBuf,
}

impl Hook for AppendOnlyLog {
    fn invoke(&self, operation: &Operation) {
        let mut operation_log = format!(
            "timestamp={} operation={} level={} key=\"{}\"",
            timestamp_to_rfc3339(&operation.timestamp),
            operation.name,
            operation.level,
            operation.key,
        );

        if let Some(value) = &operation.value {
            operation_log.push_str(&format!(" value=\"{}\"", value));
        }

        self.buffer.push(operation_log);
        debug!(
            "Operation {} added to log buffer for key: {}",
            operation.name, operation.key
        );
    }
}

impl AppendOnlyLog {
    fn new(path: PathBuf, flush_interval: Duration) -> Self {
        AppendOnlyLog {
            buffer: SegQueue::new(),
            flush_interval,
            path,
        }
    }

    async fn flush_logs(aof: Arc<Self>) {
        let mut file = BufWriter::new(
            OpenOptions::new()
                .append(true)
                .create(true)
                .truncate(false)
                .open(&aof.path)
                .await
                .expect("Failed to open AOF file"),
        );
        let mut interval = interval(aof.flush_interval);

        loop {
            interval.tick().await;

            let mut written_data = false;

            while let Some(log) = aof.buffer.pop() {
                if let Err(e) = file.write_all(log.as_bytes()).await {
                    error!("Failed to write log to AOF file: {}", e);
                } else {
                    written_data = true;
                }

                if let Err(e) = file.write_all(b"\n").await {
                    error!("Failed to write newline to AOF file: {}", e);
                } else {
                    written_data = true;
                }
            }

            if written_data {
                if let Err(e) = file.flush().await {
                    error!("Failed to flush AOF file: {}", e);
                }
            }
        }
    }

    pub async fn init(config: &Config) -> Arc<Self> {
        let flush_interval = Duration::from_millis(config.aof_flush_interval());
        info!(
            "Initializing AppendOnlyLog with flush interval: {:?}",
            flush_interval
        );

        let aof = Arc::new(AppendOnlyLog::new(
            config.aof_file().to_path_buf(),
            flush_interval,
        ));

        tokio::spawn(Self::flush_logs(Arc::clone(&aof)));

        aof
    }
}
