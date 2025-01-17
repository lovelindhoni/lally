use crate::utils::Operation;
pub mod aof;

pub trait Hook: Send + Sync {
    // invoke fn might be async in future xD
    fn invoke(&self, operation: &Operation);
}
