//! Relating memories across abouts without declaring anything.
//!
//! Abouts are never joined by relations, so what two abouts have to do
//! with each other is read off what they share: a dimension scope, and
//! where on one clock each memory stands inside it. This module turns a set
//! of facts — entries placed in time, with the lifecycle state they had
//! where the read stood — and the edges each about declared into two
//! deterministic readings: coordinate relations between facts of different
//! abouts, and tensions between facts a `contradicts` edge joins that both
//! still stand. Nothing here is stored and nothing is guessed.

/// The `method` a writer stamps on an equivalence it declares from a
/// `kmp_relate` proposal, followed by the signals that proposed it:
/// `kmp_relate:identifier+entity`. The kernel admits a relation across
/// abouts only when it carries this, with why and evidence beside it.
pub const DECLARED_FROM_RELATE_METHOD: &str = "kmp_relate";

mod coordinate_relation;
mod declared_edge;
mod proposal;
mod related_fact;
mod relations;
mod tension;

pub use coordinate_relation::{CoordinateRelation, CoordinateRelationKind};
pub use declared_edge::DeclaredEdge;
pub use proposal::{MAX_PROPOSALS_PER_FACT, ProposalSignal, ProposedLink, cap_proposals_per_fact};
pub use related_fact::{FactState, RelatedFact};
pub use relations::{MAX_COORDINATE_RELATIONS, Relations, relate};
pub use tension::Tension;
