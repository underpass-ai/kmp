//! JSON representation and byte budgeting for optional consolidated views.
mod projected_claim;
mod projection;
mod view_revision;
pub use projected_claim::ProjectedClaim;
pub use projection::{ProjectedConsolidation, project};
pub use view_revision::ViewRevision;

#[cfg(test)]
mod tests;
