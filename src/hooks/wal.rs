use crossbeam::queue::SegQueue;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::time::{interval, Duration};

use super::Hook;
use crate::config::Config;
use crate::types::Operation;
use crate::utils::LallyStamp;

pub struct WriteAheadLogging {
    buffer: SegQueue<String>,
    flush_interval: Duration,
    path: PathBuf,
}

impl Hook for WriteAheadLogging {
    fn invoke(&self, operation: &Operation) {
        let mut operation_log = format!(
            "timestamp={} operation={} level={} key=\"{}\"",
            LallyStamp::to_rfc3339(&operation.timestamp),
            operation.name,
            operation.level,
            operation.key,
        );

        if let Some(value) = &operation.value {
            operation_log.push_str(&format!(" value=\"{}\"", value));
        }

        self.buffer.push(operation_log);
    }
}

impl WriteAheadLogging {
    fn new(path: PathBuf, flush_interval: Duration) -> Self {
        WriteAheadLogging {
            buffer: SegQueue::new(),
            flush_interval,
            path,
        }
    }

    async fn flush_logs(wal: Arc<Self>) {
        let mut file = BufWriter::new(
            OpenOptions::new()
                .append(true)
                .create(true)
                .truncate(false)
                .open(&wal.path)
                .await
                .expect("Failed to open WAL file"),
        );
        let mut interval = interval(wal.flush_interval);

        loop {
            interval.tick().await;

            while let Some(log) = wal.buffer.pop() {
                if let Err(e) = file.write_all(log.as_bytes()).await {
                    eprintln!("Failed to write log: {}", e);
                }
                if let Err(e) = file.write_all(b"\n").await {
                    eprintln!("Failed to write newline: {}", e);
                }
            }

            if let Err(e) = file.flush().await {
                eprintln!("Failed to flush WAL file: {}", e);
            }
        }
    }

    pub async fn init(config: &Config) -> Arc<Self> {
        // from config rn

        let flush_interval = Duration::from_millis(100);

        let wal = Arc::new(WriteAheadLogging::new(
            config.log_path().to_path_buf(),
            flush_interval,
        ));

        // i might move this to a seperate task manager like thing
        tokio::spawn(Self::flush_logs(Arc::clone(&wal)));

        wal
    }
}
