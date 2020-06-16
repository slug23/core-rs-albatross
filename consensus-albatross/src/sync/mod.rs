use futures::Future;
use std::sync::Arc;

pub use self::history::*;
use crate::consensus::Consensus;
use crate::error::SyncError;

mod history;

pub trait SyncProtocol: Sized + Send + Sync + 'static {
    fn perform_sync(
        &self,
        consensus: Arc<Consensus<Self>>,
    ) -> Box<dyn Future<Item = (), Error = SyncError> + 'static + Send>;

    fn is_established(&self) -> bool;
}
