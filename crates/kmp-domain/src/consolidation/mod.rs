//! Optional writer-declared consolidated views. Equality here is equality of
//! declarations, never a claim that the kernel understood their source prose.
mod assertion;
mod clocks;
pub use clocks::ConsolidationClocks;
mod claim;
mod command;
mod policy;
mod selection;
mod source;
mod store;
mod view;

pub use assertion::ConsolidationAssertion;
pub use axis::ConsolidationAxis;
pub use claim::ClaimIdentity;
pub use command::ConsolidationWrite;
pub use epistemic_status::EpistemicStatus;
pub use group::ConsolidatedClaim;
pub use polarity::ClaimPolarity;
pub use policy::{MAX_SOURCE_BYTES, MAX_SOURCE_RELATIONS, MAX_SOURCES, consolidate};
pub use read::ConsolidationRead;
pub use selection::ConsolidationSelection;
pub use source::ConsolidationSource;
pub use store::{ConsolidationFuture, ConsolidationStore};
pub use view::ConsolidatedView;

#[cfg(test)]
mod selection_tests;

mod axis;
mod epistemic_status;
mod group;
mod polarity;
mod read;

mod status;
pub use status::ConsolidationReadStatus;
