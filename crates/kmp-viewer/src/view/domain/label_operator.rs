//! The kernel's closed selector vocabulary, as the intent schema exposes it.

/// One of the four ways a label predicate reads an entry's labels — the
/// same four `dimensions.selectors` takes on every kernel read. A string
/// outside this vocabulary never becomes a `LabelOperator`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelOperator {
    /// One of the entry's values under the key is among the named values.
    In,
    /// None of them is; an entry without the key passes.
    NotIn,
    /// The key is present.
    Exists,
    /// The key is absent.
    NotExists,
}

impl LabelOperator {
    /// The advertised names, in schema order.
    pub const NAMES: [&'static str; 4] = ["in", "notin", "exists", "notexists"];

    /// Parses an advertised operator; anything else is outside the vocabulary.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "in" => Some(Self::In),
            "notin" => Some(Self::NotIn),
            "exists" => Some(Self::Exists),
            "notexists" => Some(Self::NotExists),
            _ => None,
        }
    }

    /// The advertised name of this operator.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::In => "in",
            Self::NotIn => "notin",
            Self::Exists => "exists",
            Self::NotExists => "notexists",
        }
    }

    /// Whether the operator compares values; the other two only ask for
    /// the key.
    pub fn takes_values(self) -> bool {
        matches!(self, Self::In | Self::NotIn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_operator_parses_back_to_itself_and_the_vocabulary_is_closed() {
        for name in LabelOperator::NAMES {
            let operator = LabelOperator::parse(name).expect("an advertised operator parses");
            assert_eq!(operator.as_str(), name);
        }
        assert_eq!(LabelOperator::parse("like"), None);
        assert!(LabelOperator::In.takes_values());
        assert!(!LabelOperator::Exists.takes_values());
    }
}
