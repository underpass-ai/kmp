use kmp_domain::NodeRelationProjection;
use kmp_proto_mapping::v1beta1::{LanguageSignals, RelationClock};

/// What the sidecar holds of one relation the ask's selection keeps: what
/// it says to the about's language, and what it says to the about's
/// lifecycle (its latest instant, its `valid_until`, its sequence), which an
/// ask answered from the index stands on (DESIGN L6, P13).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct KeptRelation {
    pub(super) signals: LanguageSignals,
    pub(super) clock: RelationClock,
}

impl KeptRelation {
    pub(super) fn of(edge: &NodeRelationProjection) -> Self {
        Self {
            signals: LanguageSignals::of_explanation(&edge.explanation),
            clock: RelationClock::of(&edge.explanation),
        }
    }
}
