use std::fmt::{Display, Formatter};

/// One place in the tree that carries the release version, together with the
/// exact text the version change must leave there. Sources spell the version
/// differently — a bare `0.6.1`, a `v0.6.1` catalog ref, a full asset URL — so
/// each source carries its own expectation rather than a shared format rule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionSource {
    label: String,
    expected: String,
    found: String,
}

impl VersionSource {
    pub fn new(
        label: impl Into<String>,
        expected: impl Into<String>,
        found: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            expected: expected.into(),
            found: found.into(),
        }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn agrees(&self) -> bool {
        self.expected == self.found
    }
}

impl Display for VersionSource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} is {}, not {}",
            self.label, self.found, self.expected
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_disagreeing_source_names_both_sides() {
        let source = VersionSource::new(".claude-plugin/marketplace.json ref", "v0.6.1", "v0.6.0");

        assert!(!source.agrees());
        assert_eq!(
            source.to_string(),
            ".claude-plugin/marketplace.json ref is v0.6.0, not v0.6.1"
        );
    }

    #[test]
    fn an_agreeing_source_needs_no_report() {
        assert!(VersionSource::new("Cargo.toml version", "0.6.1", "0.6.1").agrees());
    }
}
