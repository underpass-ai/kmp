use crate::DomainError;

/// Classification of the system or actor that produced a fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceKind {
    Human,
    Agent,
    Projection,
    Derived,
    Unknown,
}

impl SourceKind {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value.trim() {
            "human" => Ok(Self::Human),
            "agent" => Ok(Self::Agent),
            "projection" => Ok(Self::Projection),
            "derived" => Ok(Self::Derived),
            "unknown" => Ok(Self::Unknown),
            other => Err(DomainError::InvalidState(format!(
                "invalid source_kind `{other}`"
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Agent => "agent",
            Self::Projection => "projection",
            Self::Derived => "derived",
            Self::Unknown => "unknown",
        }
    }

    pub fn all() -> &'static [Self] {
        &[
            Self::Human,
            Self::Agent,
            Self::Projection,
            Self::Derived,
            Self::Unknown,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_roundtrip() {
        for kind in SourceKind::all() {
            let parsed = SourceKind::parse(kind.as_str()).expect("valid");
            assert_eq!(&parsed, kind);
        }
    }

    #[test]
    fn parse_invalid_returns_error() {
        assert!(SourceKind::parse("bogus").is_err());
    }
}
