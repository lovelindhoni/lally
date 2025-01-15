use crate::hooks::Hook;
use crate::utils::Operation;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Default)]
pub struct Hooks {
    // TODO: change Mutex to RwLock
    hooks: Mutex<Vec<Arc<dyn Hook>>>,
}

impl Hooks {
    pub async fn register(&self, hook: Arc<dyn Hook>) {
        let mut lock_hooks = self.hooks.lock().await;
        lock_hooks.push(hook);
    }
    pub async fn invoke_all(&self, operation: &Operation) {
        let hooks = self.hooks.lock().await;
        // might want to make this asynchroousn concurrently
        for hook in hooks.iter() {
            hook.invoke(operation);
        }
    }
}
