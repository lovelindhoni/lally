mod cluster;
mod config;
mod hooks;
mod http_server;
mod lally;
mod timestamp;
mod utils;

use crate::cluster::GrpcServer;
use crate::config::Config;
use crate::hooks::aof::AppendOnlyLog;
use crate::lally::Lally;
use std::sync::Arc;
use tracing::{error, info, warn};

const LOGO: &str = r#"
 
 ██╗      █████╗ ██╗     ██╗  ██╗   ██╗
 ██║     ██╔══██╗██║     ██║  ╚██╗ ██╔╝
 ██║     ███████║██║     ██║   ╚████╔╝
 ██║     ██╔══██║██║     ██║    ╚██╔╝
 ███████╗██║  ██║███████╗███████╗██║
 ╚══════╝╚═╝  ╚═╝╚══════╝╚══════╝╚═╝
 
 An In-Memory key-value DB, trying its best
 to achieve availability
 "#;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().without_time().init();

    println!("{LOGO}");

    match Config::new().await {
        Ok(config) => {
            info!("Configuration loaded successfully.");
            let lally = match Lally::new(&config).await {
                Ok(lally) => lally,
                Err(e) => {
                    error!("Failed to initialize Lally: {}", e);
                    return;
                }
            };

            info!("Starting gRPC server...");
            if let Err(e) = GrpcServer::run(Arc::clone(&lally), &config).await {
                error!("Failed to start gRPC server: {}", e);
                return;
            }

            // Joining the cluster if a seed node addr is provided
            match config.seed_node() {
                Some(addr) => {
                    info!("Attempting to join cluster on address: {}", addr);
                    match lally.pool.join(addr.to_string()).await {
                        Ok(store_data) => {
                            info!("Successfully joined cluster.");
                            lally.store.import_store(store_data);
                        }
                        Err(e) => {
                            error!("Failed to join cluster: {}", e);
                        }
                    };
                }
                None => {
                    warn!("No seed node address provided; this node will act as the first node in the cluster.");
                }
            }

            let wal_hook = AppendOnlyLog::init(&config).await;
            lally.hooks.register(wal_hook).await;

            if let Err(e) = http_server::run(Arc::clone(&lally), config).await {
                error!("Failed to run HTTP server: {}", e);
                return;
            }
        }
        Err(e) => {
            error!("Failed to load configuration: {}", e);
        }
    }
}
