mod cluster;
mod config;
mod hooks;
mod http_server;
mod lally;
mod utils;

use crate::cluster::GrpcServer;
use crate::config::Config;
use crate::hooks::wal::WriteAheadLogging;
use crate::lally::Lally;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let config = Config::new().await.unwrap();
    let lally = Lally::new(&config).await.unwrap();

    GrpcServer::run(Arc::clone(&lally)).await;
    match config.ip() {
        Some(ip) => {
            let join_response = lally.cluster.join(ip.to_string()).await;
            if let Ok(store_data) = join_response {
                lally.store.import_store(store_data);
            }
        }
        None => println!("no seed node address given, this would act as the first node of cluster"),
    };

    let wal_hook = WriteAheadLogging::init(&config).await;
    lally.hooks.register(wal_hook).await;

    http_server::run(Arc::clone(&lally), config).await.unwrap();

    // println!("Hello, world!");
}
