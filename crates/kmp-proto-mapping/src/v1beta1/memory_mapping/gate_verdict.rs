use std::collections::BTreeSet;

use kmp_proto::v1beta1::{AnswerStatus, UnknownReason};

/// What the anchored gate decided about one question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GateVerdict {
    pub(super) status: AnswerStatus,
    /// Unspecified unless the status is unknown.
    pub(super) reason: UnknownReason,
    /// The ids of the evidence the answer cites, in rank order.
    pub(super) core: Vec<String>,
    /// What the question asked and the cited memories do not state, in the
    /// reader's words.
    pub(super) missing: Vec<String>,
}

impl GateVerdict {
    pub(super) fn unknown(reason: UnknownReason, missing: Vec<String>) -> Self {
        Self {
            status: AnswerStatus::Unknown,
            reason,
            core: Vec::new(),
            missing,
        }
    }

    /// The citations, as a set.
    pub(super) fn cited(&self) -> BTreeSet<&str> {
        self.core.iter().map(String::as_str).collect()
    }
}
