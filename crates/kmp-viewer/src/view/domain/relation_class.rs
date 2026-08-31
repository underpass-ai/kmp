//! The domain's closed relation-class vocabulary, as exposed by the intent
//! schema. Unlike dimensions and telemetry series, these are not store-local.

/// One of the six ways KMP relates memories. A string outside this vocabulary
/// never becomes a `RelationClass`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationClass {
    /// It made this happen.
    Causal,
    /// It proves or undermines this.
    Evidential,
    /// It is why this was wanted.
    Motivational,
    /// It is a step toward this.
    Procedural,
    /// It bounds this.
    Constraint,
    /// It merely organizes this.
    Structural,
}

impl RelationClass {
    /// The advertised names, in schema order.
    pub const NAMES: [&'static str; 6] = [
        "causal",
        "evidential",
        "motivational",
        "procedural",
        "constraint",
        "structural",
    ];

    /// Parses an advertised class; anything else is outside the vocabulary.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "causal" => Some(RelationClass::Causal),
            "evidential" => Some(RelationClass::Evidential),
            "motivational" => Some(RelationClass::Motivational),
            "procedural" => Some(RelationClass::Procedural),
            "constraint" => Some(RelationClass::Constraint),
            "structural" => Some(RelationClass::Structural),
            _ => None,
        }
    }

    /// The advertised name of this class.
    pub fn as_str(self) -> &'static str {
        match self {
            RelationClass::Causal => "causal",
            RelationClass::Evidential => "evidential",
            RelationClass::Motivational => "motivational",
            RelationClass::Procedural => "procedural",
            RelationClass::Constraint => "constraint",
            RelationClass::Structural => "structural",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_class_parses_back_to_itself_and_the_vocabulary_is_closed() {
        for name in RelationClass::NAMES {
            let class = RelationClass::parse(name).expect("an advertised class parses");
            assert_eq!(class.as_str(), name);
        }
        assert_eq!(RelationClass::parse("telepathic"), None);
    }
}
