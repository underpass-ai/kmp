pub mod bundle_node;
pub mod bundle_node_detail;
pub mod bundle_relationship;
pub mod kmp_bundle;
pub mod kmp_stats;
pub mod labels;
mod proof_dependency_group;
pub mod relate;
mod relationship_path;
pub mod temporal_memory;

pub use bundle_node::BundleNode;
pub use bundle_node_detail::BundleNodeDetail;
pub use bundle_relationship::BundleRelationship;
pub use kmp_bundle::KmpBundle;
pub use kmp_stats::KmpStats;
pub use labels::{
    INTENDED_NEW_LABEL_METADATA_KEY, LabelResemblance, ResemblanceKind, label_resemblances,
    labels_by_entry, normalized_label_token,
};
pub use proof_dependency_group::ProofDependencyGroup;
pub use relate::{
    CoordinateRelation, CoordinateRelationKind, DECLARED_FROM_RELATE_METHOD, DeclaredEdge,
    FactState, MAX_COORDINATE_RELATIONS, MAX_PROPOSALS_PER_FACT, ProposalSignal, ProposedLink,
    RelatedFact, Relations, Tension, cap_proposals_per_fact, relate,
};
pub use relationship_path::directed_relationship_path;
pub use temporal_memory::{
    TemporalEntry, TemporalEntrySelection, TemporalMemoryTraversal, TemporalTraversalRequest,
    TemporalTraversalResult, compare_temporal_coordinates, compare_temporal_instants,
    rfc3339_from_epoch_seconds, temporal_instant_nanos, temporal_instant_rfc3339,
};

mod bounded_trace_search;
mod trace_route;
mod trace_search_limits;
mod trace_search_request;
mod trace_search_result;
mod trace_search_stop;
pub use bounded_trace_search::bounded_trace_search;
pub use trace_route::TraceRoute;
pub use trace_search_limits::TraceSearchLimits;
pub use trace_search_request::TraceSearchRequest;
pub use trace_search_result::TraceSearchResult;
pub use trace_search_stop::TraceSearchStop;

mod temporal_read_window;
pub use temporal_read_window::{TemporalReadWindow, temporal_clock_instant};

mod trace_read_budget;
mod trace_relation_step;
mod trace_temporal_admission;
pub use trace_relation_step::TraceRelationStep;
mod trace_candidate_frontier;
mod trace_path_state;

mod select_trace_material;
mod trace_material_catalog;
mod trace_material_result;
mod trace_material_selection;
mod trace_material_set;
mod trace_material_state;
mod trace_proof_requirement;
pub use select_trace_material::select_trace_material;
pub use trace_material_result::TraceMaterialResult;
pub use trace_material_selection::TraceMaterialSelection;
pub use trace_proof_requirement::TraceProofRequirement;

mod trace_dimension_policy;
mod trace_node_expansion;
mod trace_pending_states;
mod trace_routing_stats;
pub use trace_dimension_policy::TraceDimensionPolicy;
pub use trace_routing_stats::TraceRoutingStats;

mod evidence_paths;
pub use evidence_paths::{
    EvidenceMissingWitness, EvidencePathBinding, EvidencePathBindings, EvidencePathCandidate,
    EvidencePathGroup, EvidencePathRequest, EvidencePathResult, EvidencePathRole,
    EvidencePathStatus, search_evidence_paths,
};

mod author_node_card;
mod node_card_expectation;
pub mod node_card_policy;
mod node_card_rejection;
pub mod trace_body_admission;
mod trace_body_options;
mod trace_manifest;
pub use author_node_card::AuthorNodeCard;
pub use node_card_expectation::NodeCardExpectation;
pub use node_card_rejection::NodeCardRejection;
pub use trace_body_admission::TraceBodyAdmission;
pub use trace_body_options::{MAX_EXPANSION_REFS, TraceBodyOptions};
pub use trace_manifest::TraceManifestDigest;

mod materialize_trace_proof;
mod trace_proof_object;
mod trace_proof_result;
pub use trace_proof_object::TraceProofObject;
pub use trace_proof_result::TraceProofResult;
