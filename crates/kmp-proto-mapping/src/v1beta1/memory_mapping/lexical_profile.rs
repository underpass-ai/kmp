use kmp_domain::language::KERNEL_LANGUAGE;

use super::language_signals::LanguageSignals;
use super::morphology::Morphology;

/// How an about's candidates are read into terms: the language they are
/// stemmed in and whether they also read as the alias terms they spell (the
/// anchored ask gate). Two readings under different profiles count different
/// terms, so the sidecar keeps the profile its rows were read under.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LexicalProfile {
    language: Option<String>,
    aliased: bool,
}

impl LexicalProfile {
    pub fn new(language: Option<String>, aliased: bool) -> Self {
        Self { language, aliased }
    }

    /// The language `search_language` would read from an about whose texts
    /// carry `signals` and of whose memories `summaries` carry a linted
    /// English search summary: its own, or the kernel's search language when
    /// its own cannot be read and a summary is there to land on.
    pub fn decide_language(signals: &LanguageSignals, summaries: u64) -> Option<String> {
        signals
            .language()
            .map(str::to_string)
            .or_else(|| (summaries > 0).then(|| KERNEL_LANGUAGE.to_string()))
    }

    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    pub fn aliased(&self) -> bool {
        self.aliased
    }

    pub(super) fn morphology(&self) -> Morphology {
        Morphology::for_language(self.language.as_deref())
    }
}
