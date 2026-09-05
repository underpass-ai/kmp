//! The write bounded context: a caller's intent, validated and compiled
//! to canonical ingest. Policies refuse what only the caller can fix;
//! relation quality is judged, never guessed; and the plan says what to
//! read back.

pub(crate) mod accepted_counts;
pub(crate) mod arguments;
pub(crate) mod coordinates;
pub(crate) mod existing_entry;
pub(crate) mod generated_ref;
pub(crate) mod ingest_arguments;
pub(crate) mod ingest_change;
pub(crate) mod ingest_plan;
pub(crate) mod ingest_planner;
pub(crate) mod ingest_validation;
pub(crate) mod plan;
mod planner;
mod planner_audit;
mod planner_labels_audit;
pub(crate) mod read_context;
pub(crate) mod relabel_plan;
mod relabel_planner;
mod relabel_planner_audit;
pub(crate) mod relabel_results;
pub(crate) mod relate_proposal;
pub(crate) mod relation_quality;
pub(crate) mod relations;
pub(crate) mod results;
mod search_summary;
pub(crate) mod summary_planner;
mod writer_identity_audit;
pub(crate) mod writer_label;

pub(crate) use ingest_plan::KmpIngestPlan;
pub(crate) use ingest_planner::build_ingest_plan;
pub(crate) use ingest_validation::reject_refs_outside_about;
pub(crate) use planner::build_write_plan_with_root;
pub(crate) use relabel_planner::build_relabel_plan;
pub(crate) use relabel_results::relabel_result;
pub(crate) use results::{write_commit_result, write_dry_run_result};
pub(crate) use summary_planner::{build_summary_plan, is_summary_write, summary_target};
