use crate::serving::environment::{LEXICAL_INDEX_ENV, optional_env_string};

/// What the embedded backend does with the lexical sidecar
/// (`lexical-index.sqlite3`, DESIGN L6), chosen once per process by
/// `KMP_LEXICAL_INDEX`.
///
/// On by default (P13): an ask the sidecar can hold is answered from the
/// candidates its postings reach, byte for byte what reading the whole about
/// gives on every corpus measured, and every other ask as before; the first
/// ask of an about builds its index (31 s at 10^5 entries). `shadow` follows
/// the store's log and compares the sidecar with every ask, never answering
/// one (P12). `verify` answers as before and also answers from the index,
/// logging whether the two agree. `off` never opens it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LexicalIndexMode {
    /// The sidecar is never opened, followed or compared.
    Off,
    /// Maintained beside the store and compared with every ask. A
    /// measurement mode: only debug and `eval-hooks` builds can select it.
    #[cfg_attr(not(any(debug_assertions, feature = "eval-hooks")), allow(dead_code))]
    Shadow,
    /// Answers the asks it holds from the postings (P13).
    On,
    /// Answers as before, also answers from the postings, and logs whether
    /// the two agree. A measurement mode, like [`Self::Shadow`].
    #[cfg_attr(not(any(debug_assertions, feature = "eval-hooks")), allow(dead_code))]
    Verify,
}

impl LexicalIndexMode {
    /// What a process runs with when `KMP_LEXICAL_INDEX` names nothing it
    /// knows.
    pub(crate) const DEFAULT: Self = Self::On;

    /// The mode a value of `KMP_LEXICAL_INDEX` names, if any: the on/off
    /// words every switch reads, and the two measurement modes in debug and
    /// `eval-hooks` builds only.
    pub(crate) fn named(value: &str) -> Option<Self> {
        if let Some(on) = crate::environment::on_off::parse_on_off(value) {
            return Some(if on { Self::On } else { Self::Off });
        }
        match value.trim().to_ascii_lowercase().as_str() {
            #[cfg(any(debug_assertions, feature = "eval-hooks"))]
            "shadow" => Some(Self::Shadow),
            #[cfg(any(debug_assertions, feature = "eval-hooks"))]
            "verify" => Some(Self::Verify),
            _ => None,
        }
    }

    /// The mode `KMP_LEXICAL_INDEX` asks for. An unknown value never gets
    /// here: startup refuses it ([`crate::environment::check_environment`]),
    /// and a caller that skipped the check gets the default.
    pub(crate) fn from_env() -> Self {
        optional_env_string(LEXICAL_INDEX_ENV)
            .as_deref()
            .and_then(Self::named)
            .unwrap_or(Self::DEFAULT)
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
    fn the_index_answers_unless_turned_off() {
        assert_eq!(LexicalIndexMode::DEFAULT, LexicalIndexMode::On);
        assert!(LexicalIndexMode::DEFAULT.is_open() && LexicalIndexMode::DEFAULT.reads_postings());
        assert!(!LexicalIndexMode::Off.is_open());
        assert_eq!(
            LexicalIndexMode::named(" Shadow "),
            Some(LexicalIndexMode::Shadow)
        );
        assert!(LexicalIndexMode::Shadow.is_open());
        assert_eq!(LexicalIndexMode::named("off"), Some(LexicalIndexMode::Off));
        assert_eq!(LexicalIndexMode::named("0"), Some(LexicalIndexMode::Off));
        assert_eq!(
            LexicalIndexMode::named("FALSE"),
            Some(LexicalIndexMode::Off)
        );
        assert_eq!(LexicalIndexMode::named("on"), Some(LexicalIndexMode::On));
        assert_eq!(LexicalIndexMode::named("1"), Some(LexicalIndexMode::On));
        assert!(LexicalIndexMode::On.reads_postings() && !LexicalIndexMode::On.shadows());
        assert!(LexicalIndexMode::Verify.reads_postings());
        assert_eq!(LexicalIndexMode::named("maybe"), None);
    }
}
