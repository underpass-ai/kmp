use crate::PortError;

use super::LifecycleNeighbours;

/// Where a lifecycle walk reads its links.
///
/// Both reads are bounded by the source: a store answers each from an index
/// range, a bundle from a map it built once.
pub trait LifecycleLinkSource {
    /// The links whose `older` end is `node`: what took over from it.
    fn newer_than(&self, node: &str) -> Result<LifecycleNeighbours, PortError>;

    /// The links whose `newer` end is `node`: what it took over from.
    fn older_than(&self, node: &str) -> Result<LifecycleNeighbours, PortError>;
}
