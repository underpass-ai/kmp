//! The published consumer contract of the embedded Rehydration Kernel.
//!
//! Sibling of `rehydration-plugin-api`, pointing the other way: that crate is
//! what a plugin may know about the kernel, this one is what an embedding
//! product may know. It holds plain views, a capability report, an error
//! vocabulary and two traits — recall and record, held separately on purpose —
//! no domain aggregates, no ports, no storage, no transports. A consumer that
//! compiles against this crate alone can be tested with a stub and swapped
//! onto any implementation that honours the contract.
//!
//! Versioned deliberately. [`CONTRACT_VERSION`] moves when the meaning of this
//! surface changes, independently of the kernel's own release number: two
//! builds of one release can differ in features, and a consumer that guessed
//! capabilities from a version string would find out mid-run. Consumers check
//! [`ApiCapabilities`] at startup instead.
//!
//! The kernel owns this contract, and its vocabulary is the kernel's — abouts,
//! memory, wake, ask, record. A consuming product maps these to its own terms
//! at its own boundary; nothing of any consumer's vocabulary appears here.

mod api_capabilities;
mod api_error;
mod memory_recall_api;
mod memory_record_api;
mod memory_record_requests;
mod memory_record_views;
mod memory_requests;
mod memory_views;

pub use api_capabilities::ApiCapabilities;
pub use api_error::ApiError;
pub use memory_recall_api::MemoryRecallApi;
pub use memory_record_api::MemoryRecordApi;
pub use memory_record_requests::{
    MemoryCoordinateSpec, MemoryDimensionSpec, MemoryEntrySpec, MemoryEvidenceSpec,
    MemoryProvenanceSpec, MemoryRecordRequest, MemoryRelationSpec,
};
pub use memory_record_views::RecordedMemoryView;
pub use memory_requests::{MemoryAnswerPolicy, MemoryAskRequest, MemoryTier, MemoryWakeRequest};
pub use memory_views::{
    MemoryDetailView, MemoryNodeView, MemoryRecallView, MemoryRelationshipView, RenderedMemoryView,
};

/// The revision of this contract.
///
/// Moves on meaning, not on release: adding a capability keeps the version,
/// changing what an existing field or method means raises it.
pub const CONTRACT_VERSION: u32 = 1;
