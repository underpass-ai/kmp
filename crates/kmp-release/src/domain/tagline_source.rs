use std::fmt::{Display, Formatter};

/// One place in the tree that describes the product in a sentence, and the
/// sentence every such place has to open with. A storefront reads one of
/// these files and nothing else, so a description that drifts in one of them
/// is a different product on that storefront.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaglineSource {
    label: String,
    tagline: String,
    found: String,
}

impl TaglineSource {
    pub fn new(
        label: impl Into<String>,
        tagline: impl Into<String>,
        found: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            tagline: tagline.into(),
            found: found.into(),
        }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    /// A source may say more than the tagline — a plugin manifest goes on to
    /// name what the plugin wires — but it has to say the tagline first.
    pub fn agrees(&self) -> bool {
        self.found.starts_with(&self.tagline)
    }
}

impl Display for TaglineSource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} opens with {:?}, not {:?}",
            self.label, self.found, self.tagline
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_may_extend_the_tagline_but_not_replace_it() {
        let tagline = "Local-first agent memory that preserves why";
        assert!(TaglineSource::new("a", tagline, tagline).agrees());
        assert!(TaglineSource::new("a", tagline, format!("{tagline}: wires the server")).agrees());
        let drifted =
            TaglineSource::new("plugin.json description", tagline, "Memory with evidence");
        assert!(!drifted.agrees());
        assert_eq!(
            drifted.to_string(),
            "plugin.json description opens with \"Memory with evidence\", not \"Local-first agent memory that preserves why\""
        );
    }
}
