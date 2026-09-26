//! The declared lifecycle of a memory: what it replaced and what replaced it.
//!
//! Three relation types say that one memory takes over from another —
//! `supersedes`, `corrects` and `updates_state` — and all three run from the
//! newer memory to the older one. [`LifecycleChain::walk`] follows them in
//! both directions from one memory, bounded in depth and size, cutting
//! cycles, and reports where the succession forks. It reads links through a
//! [`LifecycleLinkSource`]: the embedded store answers from its
//! `relations_*_by_kind` index, one range per hop and type, so a chain of
//! depth D costs O(D·log N) reads whatever the size of the store; a bundle
//! already in memory answers from its own relationships.

mod adjacency_lifecycle_links;
mod lifecycle_chain;
mod lifecycle_fork;
mod lifecycle_link;
mod lifecycle_link_source;
mod lifecycle_neighbours;
mod lifecycle_relation;
mod lifecycle_side;
mod lifecycle_step;

pub use adjacency_lifecycle_links::AdjacencyLifecycleLinks;
pub use lifecycle_chain::{
    LifecycleChain, MAX_LIFECYCLE_DEPTH, MAX_LIFECYCLE_LINKS_PER_NODE, MAX_LIFECYCLE_MEMBERS,
};
pub use lifecycle_fork::LifecycleFork;
pub use lifecycle_link::LifecycleLink;
pub use lifecycle_link_source::LifecycleLinkSource;
pub use lifecycle_neighbours::LifecycleNeighbours;
pub use lifecycle_relation::LifecycleRelation;
pub use lifecycle_side::LifecycleSide;
pub use lifecycle_step::LifecycleStep;

#[cfg(test)]
mod lifecycle_chain_tests;
