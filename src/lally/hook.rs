use crate::hooks::Hook;
use crate::utils::Operation;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, span, Level};

#[derive(Default)]
pub struct Hooks {
    hooks: RwLock<Vec<Arc<dyn Hook>>>,
}

impl Hooks {
    pub async fn register(&self, hook: Arc<dyn Hook>) {
        let mut lock_hooks = self.hooks.write().await;
        lock_hooks.push(hook);
        info!(
            "Hook registered successfully, total hooks: {}",
            lock_hooks.len()
        );
    }

    pub async fn invoke_all(&self, operation: &Operation) {
        let hooks = self.hooks.read().await;
        let trace_span = span!(Level::INFO, "HOOKS");
        let _enter = trace_span.enter();

        info!(key = %operation.key, "Invoking hooks for the {} operation", operation.name);
        // only log if hooks fails, currently the aof hook which is the only one, doesn't fail
        // (hopefully)
        for hook in hooks.iter() {
            hook.invoke(operation);
        }
    }
}
