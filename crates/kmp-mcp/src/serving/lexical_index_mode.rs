use crate::serving::environment::{LEXICAL_INDEX_ENV, optional_env_string};

/// What the embedded backend does with the lexical sidecar
/// (`lexical-index.sqlite3`, DESIGN L6), chosen once per process by
/// `KMP_LEXICAL_INDEX`.
///
/// Off by default: in shadow the sidecar answers nothing and costs about 5 %
/// per ask and a build on an about's first ask (31 s at 10^5 entries), so it
/// stays closed until something reads it. `shadow` opens it, follows the
/// store's log and compares it with every ask, never answering one. `on`
/// answers an ask it can hold from the candidates its postings reach (P13),
/// and every other ask as before. `verify` answers as before and also
/// answers from the index, logging whether the two agree byte for byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LexicalIndexMode {
    /// The sidecar is never opened, followed or compared.
    Off,
    /// Maintained beside the store and compared with every ask.
    Shadow,
    /// Answers the asks it holds from the postings (P13).
    On,
    /// Answers as before, also answers from the postings, and logs whether
    /// the two agree.
    Verify,
}

impl LexicalIndexMode {
    /// What a process runs with when `KMP_LEXICAL_INDEX` names nothing it
    /// knows.
    pub(crate) const DEFAULT: Self = Self::Off;

    /// The mode a value of `KMP_LEXICAL_INDEX` names, if any.
    pub(crate) fn named(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "shadow" => Some(Self::Shadow),
            "on" => Some(Self::On),
            "verify" => Some(Self::Verify),
            _ => None,
        }
    }

    /// The mode `KMP_LEXICAL_INDEX` asks for; an unknown value is reported
    /// and the default stands.
    pub(crate) fn from_env() -> Self {
        let Some(value) = optional_env_string(LEXICAL_INDEX_ENV) else {
            return Self::DEFAULT;
        };
        Self::named(&value).unwrap_or_else(|| {
            tracing::warn!(
                target: "kmp_mcp::lexical_index",
                value = %value,
                "unknown KMP_LEXICAL_INDEX; the lexical index stays off"
            );
            Self::DEFAULT
        })
    }

    /// Whether the sidecar is opened at all.
    pub(crate) fn is_open(self) -> bool {
        !matches!(self, Self::Off)
    }

    /// Whether an ask it holds is answered from the postings (`on`) or
    /// also answered from them to be compared (`verify`).
    pub(crate) fn reads_postings(self) -> bool {
        matches!(self, Self::On | Self::Verify)
    }

    /// Whether every ask is also compared with the ranker in shadow (P12):
    /// only in `shadow`; an ask the postings answer is its own comparison.
    pub(crate) fn shadows(self) -> bool {
        matches!(self, Self::Shadow)
    }
}

#[cfg(test)]
mod tests {
    use super::LexicalIndexMode;

    #[test]
    fn the_index_is_off_unless_asked_for() {
        assert_eq!(LexicalIndexMode::DEFAULT, LexicalIndexMode::Off);
        assert!(!LexicalIndexMode::DEFAULT.is_open());
        assert_eq!(
            LexicalIndexMode::named(" Shadow "),
            Some(LexicalIndexMode::Shadow)
        );
        assert!(LexicalIndexMode::Shadow.is_open());
        assert_eq!(LexicalIndexMode::named("off"), Some(LexicalIndexMode::Off));
        assert_eq!(LexicalIndexMode::named("on"), Some(LexicalIndexMode::On));
        assert!(LexicalIndexMode::On.reads_postings() && !LexicalIndexMode::On.shadows());
        assert!(LexicalIndexMode::Verify.reads_postings());
        assert_eq!(LexicalIndexMode::named("maybe"), None);
    }
}
