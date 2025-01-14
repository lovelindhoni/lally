pub mod connnection;
pub mod hook;
pub mod store;

use crate::config::Config;
use connnection::Connections;
use hook::Hooks;
use std::sync::Arc;
use store::Store;
use tokio::signal::ctrl_c;
use tokio::signal::unix::{signal, SignalKind};

#[derive(Clone)]
pub struct Lally {
    pub store: Arc<Store>,
    pub hooks: Arc<Hooks>,
    pub cluster: Arc<Connections>,
}

impl Lally {
    pub async fn new(config: &Config) -> Arc<Self> {
        let lally = Arc::new(Lally {
            store: Arc::new(Store::new(config.log_path()).await.unwrap()),
            hooks: Arc::new(Hooks::default()),
            cluster: Arc::new(Connections::default()),
        });
        tokio::spawn(Self::shutdown(Arc::clone(&lally)));
        lally
    }

    // i might just move this shutdown logic to a seperate task manager like module
    async fn shutdown(lally: Arc<Lally>) {
        // idk whether i should handle SIGQUIT too...
        let mut sigterm = signal(SignalKind::interrupt()).unwrap();
        tokio::select! {
            _ = ctrl_c() => {},
            _ = sigterm.recv() => {},
        }
        println!("shutdown(gracefull) started...");
        lally.cluster.leave().await;
        std::process::exit(0);
    }
}
