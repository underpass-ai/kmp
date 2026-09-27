//! The lexical index kept beside the store (DESIGN L6, option (a)): a
//! SQLite sidecar derived from the store's event log, maintained after every
//! write and before every ask, and compared in shadow with what each ask's
//! ranker measures. It does not answer asks yet (P13 does, through
//! [`crate::serving::ports::lexical_candidates::LexicalCandidates`]).

mod about_change;
mod about_reader;
mod about_rebuild;
mod about_refresh;
mod about_stats;
pub(crate) mod catch_up_report;
mod lexical_maintainer;
pub(crate) mod lexical_sidecar;
mod node_state;
mod relation_key;
mod row_writer;
mod shadow_comparison;
pub(crate) mod shadow_report;
mod sidecar_candidates;
mod sidecar_meta;
mod sqlite_lexical_sidecar;
mod term_postings;
mod working_set;

fn storage(error: impl std::fmt::Display) -> String {
    format!("lexical index: {error}")
}

#[cfg(test)]
mod upkeep_tests;
