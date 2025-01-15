use crate::utils::Operation;
pub mod aof;

pub trait Hook: Send + Sync {
    fn invoke(&self, operation: &Operation);
}
