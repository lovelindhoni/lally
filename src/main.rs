mod cli;
mod cluster;
mod hooks;
mod http_server;
mod lally;
mod types;
mod utils;

use crate::cluster::server::GrpcServer;
use crate::hooks::wal::WriteAheadLogging;
use crate::lally::Lally;
use std::sync::Arc;
use tokio::fs::{canonicalize, copy, OpenOptions};

#[tokio::main]
async fn main() {
    let conf = cli::config();
    let path = "/home/lovelindhoni/dev/projects/lally/lallylog.txt";
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

    let lally = Lally::new().await;
    match conf.ip {
        Some(ip) => {
            lally.cluster.join(ip).await;
        }
        None => println!("no seed node address given, this would act as the first node of cluster"),
    };

    let wal_hook = WriteAheadLogging::init().await;
    lally.hooks.register(wal_hook).await;

    GrpcServer::run(Arc::clone(&lally)).await;
    http_server::run(Arc::clone(&lally), port).await.unwrap();

    // println!("Hello, world!");
}
