use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;

use crate::PortError;

pub type ReadSnapshotFuture<'a, G, D> =
    Pin<Box<dyn Future<Output = Result<(G, D), PortError>> + Send + 'a>>;

/// Opens graph and detail ports over one read-only store state. All reads
/// through the returned ports, including clones, must keep that state until
/// the last clone is dropped. An error must never degrade to live reads.
pub trait ReadSnapshotProvider<G, D>: Debug + Send + Sync {
    fn open_snapshot(&self) -> ReadSnapshotFuture<'_, G, D>;
}
