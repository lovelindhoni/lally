use crate::hooks::Hook;
use crate::utils::Operation;
use std::sync::Arc;
use std::sync::RwLock;
use tracing::{debug, info, span, Level};

#[derive(Default)]
pub struct Hooks {
    hooks: RwLock<Vec<Arc<dyn Hook>>>,
}

impl Hooks {
    pub fn register(&self, hook: Arc<dyn Hook>) {
        let mut lock_hooks = self.hooks.write().expect("hooks lock poisoned");
        lock_hooks.push(hook);
        info!(
            "Hook registered successfully, total hooks: {}",
            lock_hooks.len()
        );
    }

    pub fn invoke_all(&self, operation: &Operation) {
        let hooks = self.hooks.read().expect("hooks lock poisoned");
        let trace_span = span!(Level::DEBUG, "HOOKS");
        let _enter = trace_span.enter();

        debug!(key = %operation.key, "Invoking hooks for the {} operation", operation.name);
        for hook in hooks.iter() {
            hook.invoke(operation);
        }
    }
}
