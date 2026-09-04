/// Why a label a writer is about to create resembles one the about holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResemblanceKind {
    /// The same label spelled differently: `component=kmp_viewer` beside
    /// `component=kmp-viewer`, or `Component=…` beside `component=…`.
    SameLabelSpelledDifferently,
    /// The same value, up to spelling, already stands under another key:
    /// `repo=kmp-viewer` beside `component=kmp-viewer`. Within an about a
    /// scope id names one label, so this is a reuse in the making.
    ValueUnderAnotherKey,
}

impl ResemblanceKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::SameLabelSpelledDifferently => "same_label_spelled_differently",
            Self::ValueUnderAnotherKey => "value_under_another_key",
        }
    }
}

/// A label the writer named beside the existing label it resembles, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelResemblance {
    key: String,
    value: String,
    existing_key: String,
    existing_value: String,
    kind: ResemblanceKind,
}

impl LabelResemblance {
    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn existing_key(&self) -> &str {
        &self.existing_key
    }

    pub fn existing_value(&self) -> &str {
        &self.existing_value
    }

    pub fn kind(&self) -> ResemblanceKind {
        self.kind
    }

    /// The sentence a writer gets, checkable against the catalogue.
    pub fn why(&self) -> String {
        match self.kind {
            ResemblanceKind::SameLabelSpelledDifferently => format!(
                "`{}={}` resembles `{}={}`, already in the about: the same identifier up to case and separators. Reuse the existing spelling, or insist on the new label if it means something else.",
                self.key, self.value, self.existing_key, self.existing_value
            ),
            ResemblanceKind::ValueUnderAnotherKey => format!(
                "`{}={}` resembles `{}={}`, already in the about: the same value under another key. Within an about a scope id names one label; reuse the existing label, choose a distinct value, or insist if this is a different thing.",
                self.key, self.value, self.existing_key, self.existing_value
            ),
        }
    }
}

/// A label token folded for comparison: lowercase, every run of separators
/// or whitespace read as one `-`, edges trimmed. `KMP_Viewer`,
/// `kmp-viewer` and `kmp.viewer` fold to `kmp-viewer`; `kmp-view` does not,
/// and neither does `v0.12.1` beside `v0.12.0`. This is the high threshold:
/// nothing but spelling is forgiven, so what it flags is never a guess.
pub fn normalized_label_token(text: &str) -> String {
    let mut folded = String::with_capacity(text.len());
    let mut pending_separator = false;
    for character in text.trim().chars() {
        let is_separator = character.is_whitespace()
            || matches!(
                character,
                '_' | '-' | '.' | ':' | '/' | '\\' | '+' | '@' | '~'
            );
        if is_separator {
            pending_separator = !folded.is_empty();
            continue;
        }
        if pending_separator {
            folded.push('-');
            pending_separator = false;
        }
        folded.extend(character.to_lowercase());
    }
    folded
}

/// The labels of the catalogue a candidate resembles, in catalogue order.
/// A candidate identical to a catalogue entry resembles nothing: that is a
/// reuse, which is the point.
pub fn label_resemblances<'a>(
    key: &str,
    value: &str,
    catalogue: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<LabelResemblance> {
    let folded_key = normalized_label_token(key);
    let folded_value = normalized_label_token(value);
    if folded_value.is_empty() {
        return Vec::new();
    }
    catalogue
        .into_iter()
        .filter(|(existing_key, existing_value)| {
            !(*existing_key == key && *existing_value == value)
                && normalized_label_token(existing_value) == folded_value
        })
        .map(|(existing_key, existing_value)| LabelResemblance {
            key: key.to_string(),
            value: value.to_string(),
            existing_key: existing_key.to_string(),
            existing_value: existing_value.to_string(),
            kind: if normalized_label_token(existing_key) == folded_key {
                ResemblanceKind::SameLabelSpelledDifferently
            } else {
                ResemblanceKind::ValueUnderAnotherKey
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bench for the threshold: every pair says whether the catalogue
    /// label on the right should be flagged for the candidate on the left,
    /// and why. Lower the threshold by adding pairs here and reading what
    /// moves, never by taste.
    const PAIRS: &[(&str, &str, &str, &str, Option<ResemblanceKind>)] = &[
        // (candidate key, candidate value, existing key, existing value, expected)
        (
            "component",
            "kmp_viewer",
            "component",
            "kmp-viewer",
            Some(ResemblanceKind::SameLabelSpelledDifferently),
        ),
        (
            "component",
            "KMP-Viewer",
            "component",
            "kmp-viewer",
            Some(ResemblanceKind::SameLabelSpelledDifferently),
        ),
        (
            "Component",
            "kmp-viewer",
            "component",
            "kmp-viewer",
            Some(ResemblanceKind::SameLabelSpelledDifferently),
        ),
        (
            "release",
            "v0-12-0",
            "release",
            "v0.12.0",
            Some(ResemblanceKind::SameLabelSpelledDifferently),
        ),
        (
            "release",
            "release:v0.12.0",
            "release",
            "release-v0.12.0",
            Some(ResemblanceKind::SameLabelSpelledDifferently),
        ),
        (
            "task",
            "kmp 506",
            "task",
            "kmp-506",
            Some(ResemblanceKind::SameLabelSpelledDifferently),
        ),
        (
            "repo",
            "kmp-viewer",
            "component",
            "kmp-viewer",
            Some(ResemblanceKind::ValueUnderAnotherKey),
        ),
        (
            "repo",
            "KMP_VIEWER",
            "component",
            "kmp-viewer",
            Some(ResemblanceKind::ValueUnderAnotherKey),
        ),
        ("component", "kmp-viewer", "component", "kmp-viewer", None),
        ("component", "kmp-view", "component", "kmp-viewer", None),
        (
            "component",
            "kmp-viewer-ui",
            "component",
            "kmp-viewer",
            None,
        ),
        ("release", "v0.12.1", "release", "v0.12.0", None),
        ("release", "v0.12.0", "release", "v0.11.0", None),
        ("owner", "tirso", "owners", "tirsos", None),
        ("customer", "acme", "customer", "acme-corp", None),
        ("env", "prod", "env", "production", None),
    ];

    #[test]
    fn the_bench_holds_at_the_high_threshold() {
        for (key, value, existing_key, existing_value, expected) in PAIRS {
            let found = label_resemblances(key, value, [(*existing_key, *existing_value)]);
            let kind = found.first().map(LabelResemblance::kind);
            assert_eq!(
                kind, *expected,
                "{key}={value} against {existing_key}={existing_value}"
            );
        }
    }

    #[test]
    fn folding_forgives_case_and_separators_and_nothing_else() {
        assert_eq!(normalized_label_token("KMP_Viewer"), "kmp-viewer");
        assert_eq!(normalized_label_token(" kmp.viewer "), "kmp-viewer");
        assert_eq!(
            normalized_label_token("kmp:v0.11.0:verification"),
            "kmp-v0-11-0-verification"
        );
        assert_eq!(normalized_label_token("--kmp--viewer--"), "kmp-viewer");
        assert_ne!(
            normalized_label_token("kmp-viewer"),
            normalized_label_token("kmp-view")
        );
    }

    #[test]
    fn the_why_names_both_labels_and_the_way_out() {
        let found = label_resemblances("repo", "kmp_viewer", [("component", "kmp-viewer")]);
        assert_eq!(found.len(), 1);
        let why = found[0].why();
        assert!(
            why.contains("`repo=kmp_viewer` resembles `component=kmp-viewer`"),
            "{why}"
        );
        assert!(why.contains("the same value under another key"), "{why}");
    }
}
