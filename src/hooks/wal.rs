use crossbeam::queue::SegQueue;
use std::sync::Arc;
use tokio::fs::{create_dir_all, OpenOptions};
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::time::{interval, Duration};

use super::Hook;
use crate::types::Operation;
use crate::utils::LallyStamp;

pub struct WriteAheadLogging {
    buffer: SegQueue<String>,
    flush_interval: Duration,
    path: String,
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
    fn new(path: String, flush_interval: Duration) -> Self {
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
                .open(wal.path.clone())
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

    pub async fn init() -> Arc<Self> {
        // from config rn
        let path = String::from("/home/lovelindhoni/dev/projects/lally/lallylog.txt");
        create_dir_all("/home/lovelindhoni/dev/projects/lally")
            .await
            .expect("Failed to create WAL directory");

        let flush_interval = Duration::from_millis(100);

        let wal = Arc::new(WriteAheadLogging::new(path.clone(), flush_interval));

        // i might move this to a seperate task manager like thing
        tokio::spawn(Self::flush_logs(Arc::clone(&wal)));

        wal
    }
}
