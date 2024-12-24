// #![allow(unused)]

mod cli;
mod kv_store;
mod server;
mod wal;

use cli::config;
use tokio::fs::{canonicalize, copy, OpenOptions};

#[tokio::main]
async fn main() {
    let conf = config();
    let path = "/home/lovelindhoni/dev/projects/kvr/kvrlog.txt";
    if conf.fresh && conf.path.is_some() {
        panic!("log file is not needed when starting fresh");
    }
    if conf.fresh {
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .await
            .unwrap();
    }
    if let Some(source) = conf.path {
        let canonical_source = canonicalize(source).await.unwrap();
        copy(canonical_source, path).await.unwrap();
    }
    let port = conf.port.unwrap_or(3000);
    server::run(port).await.unwrap();
}
