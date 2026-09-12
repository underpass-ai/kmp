//! Bounded recall projection: how a wake or ask answer is fitted to a byte
//! ceiling, paged by a selection-bound cursor, and carried between the
//! v1beta1 proto contract and the JSON the recall tools return.

#[path = "recall_actions.rs"]
mod actions;
mod budget;
mod core_fit;
mod cursor;
mod json_paths;
mod metadata;
mod normalization;
mod plan;
mod projection_error;
mod projection_outcome;
mod proof_value;
mod recall_output;
mod request_arguments;
mod response_value;
mod scalars;
mod text_shortening;
mod typed_recall;
mod typed_response;

pub use budget::DEFAULT_MAX_BYTES;
pub use metadata::PROJECTION_CONTRACT;
pub use projection_error::RecallProjectionError;
pub use projection_outcome::ProjectionOutcome;
pub use recall_output::{project_recall_output, project_recall_output_typed};
pub use request_arguments::requested_byte_limit;
pub use response_value::{ask_value, wake_value};
pub use typed_recall::{project_ask_response, project_wake_response};

#[cfg(test)]
#[path = "recall_action_tests.rs"]
mod action_tests;
#[cfg(test)]
mod budget_tests;
#[cfg(test)]
mod core_fit_tests;
#[cfg(test)]
mod cursor_tests;
#[cfg(test)]
#[path = "recall_pending_tests.rs"]
mod pending_tests;
#[cfg(test)]
mod plan_tests;
#[cfg(test)]
#[path = "recall_projection_rank_tests.rs"]
mod rank_tests;
#[cfg(test)]
mod response_value_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod typed_response_tests;
