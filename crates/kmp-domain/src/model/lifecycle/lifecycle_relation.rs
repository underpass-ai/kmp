/// A relation that says one memory takes over from another.
///
/// All three run from the newer memory to the older one:
/// `new -supersedes-> old` replaces the whole of it, `fix -corrects-> claim`
/// corrects a part of it, and `later -updates_state-> earlier` reports a
/// later state of what the earlier one described.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LifecycleRelation {
    Supersedes,
    Corrects,
    UpdatesState,
}

impl LifecycleRelation {
    /// Every lifecycle relation, in the order a walk reads them.
    pub const ALL: [LifecycleRelation; 3] = [
        LifecycleRelation::Supersedes,
        LifecycleRelation::Corrects,
        LifecycleRelation::UpdatesState,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Supersedes => "supersedes",
            Self::Corrects => "corrects",
            Self::UpdatesState => "updates_state",
        }
    }

    /// The lifecycle relation a stored relation type names, if it names one.
    pub fn parse(relation_type: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|relation| relation.as_str() == relation_type)
    }
}
