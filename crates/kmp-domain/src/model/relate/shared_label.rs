use std::fmt;

/// A label two facts stand in: a coordinate's dimension kind read as the
/// key and its scope, stripped of the about that namespaces it, as the
/// value.
///
/// `incident=north-outage` in two abouts is one label. The same scope under
/// another kind is another label: `owner=kmp` and `repo=kmp` share a value
/// and nothing relates through them, because a scope only means what its
/// kind says it means. Within one about a scope keeps the kind of its first
/// use, which the ingest enforces; across abouts the pair is what is
/// compared.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SharedLabel {
    key: String,
    value: String,
}

impl SharedLabel {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }

    /// The dimension kind: `task`, `agentic_process`, `incident`.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The scope as the writer named it, without the about.
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl fmt::Display for SharedLabel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}={}", self.key, self.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_reads_as_key_equals_value() {
        assert_eq!(
            SharedLabel::new("incident", "north-outage").to_string(),
            "incident=north-outage"
        );
    }

    #[test]
    fn the_same_value_under_two_kinds_is_two_labels() {
        assert_ne!(
            SharedLabel::new("owner", "kmp"),
            SharedLabel::new("repo", "kmp")
        );
    }
}
