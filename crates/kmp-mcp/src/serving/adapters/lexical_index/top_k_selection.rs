use std::collections::BTreeSet;

use kmp_proto_mapping::v1beta1::RankPrefix;

/// What one top-k round over the lexical index reads (P14): the candidates,
/// the best rank prefix left unread among those some reading could find
/// eligible, and the best direct score left unread among the carriers of a
/// bridged word, which only the bridged rescue can bring in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TopKSelection {
    pub(super) candidates: BTreeSet<String>,
    pub(super) unread: Option<RankPrefix>,
    pub(super) unread_bridged: Option<i64>,
}

impl TopKSelection {
    /// Every candidate, nothing left unread.
    pub(super) fn everything(candidates: BTreeSet<String>) -> Self {
        Self {
            candidates,
            unread: None,
            unread_bridged: None,
        }
    }

    /// Whether this round reads every candidate left: the last one.
    pub(super) fn reads_everything(&self) -> bool {
        self.unread.is_none() && self.unread_bridged.is_none()
    }
}
