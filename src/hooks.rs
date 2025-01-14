use crate::utils::Operation;
pub mod wal;

pub trait Hook: Send + Sync {
    fn invoke(&self, operation: &Operation);
}
