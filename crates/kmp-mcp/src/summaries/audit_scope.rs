use std::collections::BTreeSet;

/// Which abouts one reading of the store's summaries covers.
///
/// The words are the surface's own — `current_about`, `abouts`, `all_abouts`
/// — so an agent that already knows how `kmp_ask` reads several abouts
/// together does not learn a second vocabulary here. `AllAbouts` is the
/// explicit opt-in to a sweep of every anchor: it is a real cost on a large
/// store, and naming it is how a caller accepts that cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditScope {
    /// One about, named.
    CurrentAbout(String),
    /// A named set, read together with separate ownership.
    Abouts(Vec<String>),
    /// Every anchor the store holds.
    AllAbouts,
}

impl AuditScope {
    /// The word the surface uses for this selection.
    pub fn name(&self) -> &'static str {
        match self {
            Self::CurrentAbout(_) => "current_about",
            Self::Abouts(_) => "abouts",
            Self::AllAbouts => "all_abouts",
        }
    }

    /// Whether this reading covers the about an event is rooted at.
    pub fn covers(&self, about: &str) -> bool {
        match self {
            Self::CurrentAbout(selected) => selected == about,
            Self::Abouts(selected) => selected.iter().any(|one| one == about),
            Self::AllAbouts => true,
        }
    }

    /// The abouts this scope names, deduplicated and ordered, for a caller
    /// that echoes the selection back. Empty for `all_abouts`, which names
    /// none.
    pub fn named_abouts(&self) -> Vec<String> {
        match self {
            Self::CurrentAbout(about) => vec![about.clone()],
            Self::Abouts(abouts) => abouts
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            Self::AllAbouts => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_named_set_covers_only_what_it_names_and_all_abouts_covers_everything() {
        let set = AuditScope::Abouts(vec!["project:a".to_string(), "project:b".to_string()]);

        assert!(set.covers("project:a"));
        assert!(!set.covers("project:c"));
        assert!(AuditScope::AllAbouts.covers("project:c"));
        assert!(AuditScope::CurrentAbout("project:a".to_string()).covers("project:a"));
        assert_eq!(set.name(), "abouts");
        assert!(AuditScope::AllAbouts.named_abouts().is_empty());
    }

    #[test]
    fn a_named_set_echoes_deduplicated_and_ordered() {
        let set = AuditScope::Abouts(vec![
            "project:b".to_string(),
            "project:a".to_string(),
            "project:b".to_string(),
        ]);

        assert_eq!(set.named_abouts(), ["project:a", "project:b"]);
    }
}
