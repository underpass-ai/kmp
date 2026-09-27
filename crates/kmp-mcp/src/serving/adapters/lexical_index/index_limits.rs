/// When the lexical index is worth reading (DESIGN L6, P13), chosen per store
/// in `lexical-index.json` (`LexicalIndexConfig`).
///
/// - `max_candidate_share_percent`: an ask whose candidates cover more of the
///   about than this reads the about instead. Reading a candidate point by
///   point costs about twice what reading it with the whole about does, so
///   past roughly 40 % the index saves nothing (measured on the frozen real
///   store; decision of Tirso, 27 Sept 2026: 35 % by default, configurable).
/// - `min_about_entries`: an about with fewer entries is never indexed, and its
///   asks read the about: below it the build on the first ask costs more
///   than the asks save (measured crossover, `docs/development/lexical-sidecar.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IndexLimits {
    pub(crate) max_candidate_share_percent: u8,
    pub(crate) min_about_entries: u64,
}

impl IndexLimits {
    pub(crate) const DEFAULT: Self = Self {
        max_candidate_share_percent: 35,
        min_about_entries: 2250,
    };

    /// No size threshold: every about asked about is indexed (tests, and the
    /// `shadow` and `verify` modes, which measure every about).
    pub(crate) fn every_about(self) -> Self {
        Self {
            min_about_entries: 0,
            ..self
        }
    }

    /// Why `count` candidates of an about of `documents` are too many for the
    /// index to save anything over reading the about, if they are.
    pub(crate) fn too_many(self, count: usize, documents: u64) -> Option<&'static str> {
        (count as u64 * 100 > documents * u64::from(self.max_candidate_share_percent))
            .then_some("the candidates cover too much of the about")
    }
}

impl Default for IndexLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::IndexLimits;

    #[test]
    fn the_share_bound_is_the_configured_percentage() {
        let limits = IndexLimits::DEFAULT;
        assert_eq!(limits.too_many(35, 100), None);
        assert!(limits.too_many(36, 100).is_some());
        let wide = IndexLimits {
            max_candidate_share_percent: 60,
            ..limits
        };
        assert_eq!(wide.too_many(60, 100), None);
        assert_eq!(limits.every_about().min_about_entries, 0);
    }
}
