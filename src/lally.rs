pub mod hook;
pub mod pool;
pub mod store;

use crate::config::Config;
use anyhow::{Context, Result};
use hook::Hooks;
use pool::Pool;
use std::sync::Arc;
use store::Store;
use tokio::signal::ctrl_c;
use tokio::signal::unix::{signal, SignalKind};
use tracing::info;

#[derive(Clone)]
pub struct Lally {
    pub store: Arc<Store>,
    pub hooks: Arc<Hooks>,
    pub pool: Arc<Pool>,
}

impl Lally {
    pub async fn new(config: &Config) -> Result<Arc<Self>> {
        let lally = Arc::new(Lally {
            store: Arc::new(
                Store::new(config.aof_file())
                    .await
                    .context("Failed to create store")?,
            ),
            hooks: Arc::new(Hooks::default()),
            pool: Arc::new(Pool::default()),
        });

        // Spawn a shutdown task
        tokio::spawn(Self::shutdown(Arc::clone(&lally)));

        Ok(lally)
    }

    async fn shutdown(lally: Arc<Lally>) {
        // Set up signal handling
        let mut sigterm = signal(SignalKind::interrupt()).unwrap();

        tokio::select! {
            _ = ctrl_c() => {
                info!("Received Ctrl+C signal");
            },
            _ = sigterm.recv() => {
                info!("Received SIGINT signal");
            },
        }

        // Log graceful shutdown and perform cleanup
        info!("Graceful shutdown started...");
        lally.pool.leave().await;
        info!("Exiting Lally");
        std::process::exit(0);
    }
}
