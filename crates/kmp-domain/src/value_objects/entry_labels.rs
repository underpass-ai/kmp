use std::collections::{BTreeMap, BTreeSet};

use crate::value_objects::memory_dimension_identity::MemoryDimensionIdentity;

/// The labels one entry stands in, read as a map from key to values: each
/// coordinate's `dimension` kind is the key and its scope, stripped of the
/// about that namespaces it, is the value.
///
/// Within one about a scope keeps the kind of its first use, so a key
/// normally maps to one value; the map still holds a set, because nothing
/// in the store forbids an entry from standing in `task=a` and `task=b`
/// and a selector has to read what is there, not what is usual.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntryLabels {
    by_key: BTreeMap<String, BTreeSet<String>>,
}

impl EntryLabels {
    /// From `(dimension, scope_id)` coordinates as the bundle carries them:
    /// the scope id may be namespaced or bare, and blank pairs are skipped.
    pub fn from_coordinates<'a>(coordinates: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        Self::from_pairs(coordinates.into_iter().map(|(dimension, scope_id)| {
            (
                dimension,
                MemoryDimensionIdentity::parse(scope_id)
                    .map(|identity| identity.dimension_id().to_string())
                    .unwrap_or_else(|| scope_id.trim().to_string()),
            )
        }))
    }

    /// From `(key, value)` pairs already bare.
    pub fn from_pairs<'a>(pairs: impl IntoIterator<Item = (&'a str, impl Into<String>)>) -> Self {
        let mut by_key = BTreeMap::<String, BTreeSet<String>>::new();
        for (key, value) in pairs {
            let key = key.trim();
            let value: String = value.into();
            let value = value.trim();
            if key.is_empty() || value.is_empty() {
                continue;
            }
            by_key
                .entry(key.to_string())
                .or_default()
                .insert(value.to_string());
        }
        Self { by_key }
    }

    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }

    pub fn has_key(&self, key: &str) -> bool {
        self.by_key.contains_key(key)
    }

    pub fn values(&self, key: &str) -> Option<&BTreeSet<String>> {
        self.by_key.get(key)
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.by_key.keys().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinates_read_as_key_to_bare_values() {
        let labels = EntryLabels::from_coordinates([
            ("task", "about:project:kmp:dimension:kmp-506"),
            ("agentic_process", "kmp:v0.11.0:verification"),
            ("", "ignored"),
            ("release", "  "),
        ]);
        assert!(labels.has_key("task"));
        assert_eq!(
            labels
                .values("task")
                .map(|values| values.iter().cloned().collect::<Vec<_>>()),
            Some(vec!["kmp-506".to_string()])
        );
        assert!(labels.has_key("agentic_process"));
        assert!(!labels.has_key("release"));
        assert_eq!(
            labels.keys().collect::<Vec<_>>(),
            vec!["agentic_process", "task"]
        );
    }

    #[test]
    fn a_key_can_hold_more_than_one_value() {
        let labels = EntryLabels::from_pairs([("task", "a"), ("task", "b")]);
        assert_eq!(labels.values("task").map(BTreeSet::len), Some(2));
    }
}
