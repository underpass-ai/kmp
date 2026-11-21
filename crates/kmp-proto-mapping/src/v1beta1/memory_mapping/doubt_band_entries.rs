/// Which ways into the doubt band a store opened (DESIGN L4 4f): the band's
/// own entries (`ask-judge.json`), read with its margin, and the two of the
/// cross-language judge (`ask-cross-language.json`). All closed is no band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DoubtBandEntries {
    /// The band's entries (i)–(iii), with the margin in tenths of a BM25
    /// point under which an answer is in doubt; `None` when `ask-judge.json`
    /// is absent.
    margin_below: Option<i64>,
    /// An answer that stands only through a translation.
    translated: bool,
    /// An UNKNOWN the bridge reached memories in another language for.
    rescue: bool,
}

impl DoubtBandEntries {
    /// The band's own entries, under `margin_below` tenths.
    pub fn band(margin_below: i64) -> Self {
        Self {
            margin_below: Some(margin_below),
            ..Self::default()
        }
    }

    /// These entries and the cross-language judge's.
    pub fn with_cross_language(mut self, veto: bool, rescue: bool) -> Self {
        self.translated = veto;
        self.rescue = rescue;
        self
    }

    pub(super) fn margin_below(self) -> Option<i64> {
        self.margin_below
    }

    pub(super) fn translated(self) -> bool {
        self.translated
    }

    pub(super) fn rescue(self) -> bool {
        self.rescue
    }

    /// Whether any entry is open.
    pub fn is_open(self) -> bool {
        self.margin_below.is_some() || self.translated || self.rescue
    }
}
