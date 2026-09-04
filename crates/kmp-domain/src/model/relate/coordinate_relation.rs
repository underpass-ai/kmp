use crate::TemporalAxis;

/// How two facts of different abouts stand to each other inside a scope
/// they share. Read off their coordinates; declared by nobody.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CoordinateRelationKind {
    /// They share the scope, and neither carries an instant on the clock
    /// read to order them by.
    SharesScope,
    Before,
    After,
    /// The first holds within the span of the second, on the validity clock.
    During,
    /// They hold at once: the same instant, or overlapping spans.
    Concurrent,
    SameSequence,
    SameRank,
}

impl CoordinateRelationKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::SharesScope => "shares_scope",
            Self::Before => "before",
            Self::After => "after",
            Self::During => "during",
            Self::Concurrent => "concurrent",
            Self::SameSequence => "same_sequence",
            Self::SameRank => "same_rank",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinateRelation {
    from: String,
    to: String,
    kind: CoordinateRelationKind,
    scope_id: String,
    axis: TemporalAxis,
}

impl CoordinateRelation {
    pub fn new(
        from: impl Into<String>,
        to: impl Into<String>,
        kind: CoordinateRelationKind,
        scope_id: impl Into<String>,
        axis: TemporalAxis,
    ) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            kind,
            scope_id: scope_id.into(),
            axis,
        }
    }

    pub fn from(&self) -> &str {
        &self.from
    }

    pub fn to(&self) -> &str {
        &self.to
    }

    pub fn kind(&self) -> CoordinateRelationKind {
        self.kind
    }

    pub fn scope_id(&self) -> &str {
        &self.scope_id
    }

    pub fn axis(&self) -> TemporalAxis {
        self.axis
    }

    /// The sentence a reader gets, deterministic from the relation itself.
    pub fn why(&self) -> String {
        let scope = &self.scope_id;
        let clock = clock_name(self.axis);
        match self.kind {
            CoordinateRelationKind::SharesScope => format!(
                "Both stand in `{scope}`; neither carries an instant on the {clock} clock to order them by."
            ),
            CoordinateRelationKind::Before => format!(
                "Both stand in `{scope}`; `{}` comes before `{}` on the {clock} clock.",
                self.from, self.to
            ),
            CoordinateRelationKind::After => format!(
                "Both stand in `{scope}`; `{}` comes after `{}` on the {clock} clock.",
                self.from, self.to
            ),
            CoordinateRelationKind::During => format!(
                "Both stand in `{scope}`; `{}` holds within the span of `{}` on the validity clock.",
                self.from, self.to
            ),
            CoordinateRelationKind::Concurrent => format!(
                "Both stand in `{scope}`; `{}` and `{}` hold at once on the {clock} clock.",
                self.from, self.to
            ),
            CoordinateRelationKind::SameSequence => {
                format!("Both stand in `{scope}` at the same sequence.")
            }
            CoordinateRelationKind::SameRank => {
                format!("Both stand in `{scope}` at the same rank.")
            }
        }
    }
}

fn clock_name(axis: TemporalAxis) -> &'static str {
    match axis {
        TemporalAxis::Default => "default",
        TemporalAxis::Occurred => "occurred",
        TemporalAxis::Observed => "observed",
        TemporalAxis::Ingested => "ingested",
        TemporalAxis::Validity => "validity",
    }
}
