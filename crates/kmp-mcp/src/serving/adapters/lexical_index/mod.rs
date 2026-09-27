//! The lexical index kept beside the store (DESIGN L6, option (a)): a
//! SQLite sidecar derived from the store's event log, maintained after every
//! write and before every ask, and compared in shadow with what each ask's
//! ranker measures. With `KMP_LEXICAL_INDEX=on` an ask it can hold is
//! answered from the candidates its postings reach (P13, [`indexed_plan`]
//! and [`indexed_parts`]); every other ask reads the about.

mod about_change;
mod about_reader;
mod about_rebuild;
mod about_refresh;
mod about_stats;
pub(crate) mod catch_up_report;
mod indexed_parts;
mod indexed_plan;
pub(crate) mod indexed_read;
mod kept_relation;
mod lexical_maintainer;
pub(crate) mod lexical_sidecar;
mod node_state;
mod refreshed;
mod relation_key;
mod row_writer;
mod shadow_comparison;
pub(crate) mod shadow_report;
mod shadow_scope;
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

#[cfg(test)]
mod expansion_shadow_tests;

#[cfg(test)]
mod derivation_golden_tests;

#[cfg(test)]
mod indexed_ask_tests;
