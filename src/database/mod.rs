mod firestore;
pub use firestore::Firestore;

use crate::model::Paste;
use std::future::Future;

pub trait PasteStore: Send + Sync {
    fn create(&self, paste: Paste) -> impl Future<Output = Result<bool, anyhow::Error>> + Send;
    fn get(&self, id: &str) -> impl Future<Output = Result<Option<Paste>, anyhow::Error>> + Send;
    fn delete(
        &self,
        id: &str,
        token_hash: &str,
    ) -> impl Future<Output = Result<bool, anyhow::Error>> + Send;
}
