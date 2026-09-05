//! How a coordinate came to stand on its entry.

use crate::RelationExplanation;

/// Where a coordinate came from, read off the `contains_entry` edge that
/// holds it.
///
/// A label given at write is its own origin and carries nothing here. A
/// label put on the entry later — by `kmp_relabel`, which stamps the edge
/// with its `method`, the writer's why as the rationale, and "Relabelled by
/// … at …" as the motivation — carries all three, so a reader that draws the
/// coordinate can tell a label given at write from one stitched on
/// afterwards, and quote why.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CoordinateOrigin {
    method: Option<String>,
    rationale: Option<String>,
    motivation: Option<String>,
}

impl CoordinateOrigin {
    /// The origin an edge declares. Without a `method` the edge is the
    /// write itself, and its boilerplate rationale is not an origin worth
    /// carrying; with one, everything the edge says about it comes along.
    pub fn from_relation_explanation(explanation: &RelationExplanation) -> Self {
        let Some(method) = normalize(explanation.method()) else {
            return Self::default();
        };
        Self {
            method: Some(method),
            rationale: normalize(explanation.rationale()),
            motivation: normalize(explanation.motivation()),
        }
    }

    /// The operation that put the label there, when it was not the write.
    pub fn method(&self) -> Option<&str> {
        self.method.as_deref()
    }

    /// Why, in the words of whoever put it there.
    pub fn rationale(&self) -> Option<&str> {
        self.rationale.as_deref()
    }

    /// Who did it and when, as the projection noted it.
    pub fn motivation(&self) -> Option<&str> {
        self.motivation.as_deref()
    }

    /// Whether the coordinate came with the write.
    pub fn is_write(&self) -> bool {
        self.method.is_none()
    }
}

fn normalize(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RelationSemanticClass;

    #[test]
    fn a_label_given_at_write_is_its_own_origin() {
        let explanation = RelationExplanation::new(RelationSemanticClass::Structural)
            .with_rationale("Memory scope contains this entry.");
        let origin = CoordinateOrigin::from_relation_explanation(&explanation);
        assert!(origin.is_write());
        assert_eq!(origin.rationale(), None, "boilerplate is not an origin");
    }

    #[test]
    fn a_label_stitched_on_later_carries_method_why_and_who() {
        let explanation = RelationExplanation::new(RelationSemanticClass::Structural)
            .with_method("kmp_relabel")
            .with_rationale("The decision belongs to the issue it closed.")
            .with_optional_motivation(Some(
                "Relabelled by agent:claude at 2026-09-05T06:40:00Z.".into(),
            ));
        let origin = CoordinateOrigin::from_relation_explanation(&explanation);
        assert!(!origin.is_write());
        assert_eq!(origin.method(), Some("kmp_relabel"));
        assert_eq!(
            origin.rationale(),
            Some("The decision belongs to the issue it closed.")
        );
        assert!(
            origin
                .motivation()
                .is_some_and(|note| note.starts_with("Relabelled by"))
        );
    }
}
