//! The labels an about holds, counted for a renderer.

use std::collections::{BTreeMap, BTreeSet};

use kmp_domain::{KmpBundle, bare_label_value};
use serde::Serialize;

/// One label the about holds, as a renderer draws it: the key (`dimension`)
/// and the value, both as the coordinates spell them and as the catalogue
/// speaks, with how many entries stand in it across all time (`entries`) and
/// how many inside the projected range on the selected clock (`in_range`).
///
/// A label with nothing in range is still here. A lane that exists only
/// while the window holds one of its entries cannot show a reader that a
/// label is empty *here*; it can only fail to show the label at all. The
/// catalogue is read off the same `contains_entry` edges `kmp_wake` reads,
/// before the read's own dimension filter narrows them, so the rows a
/// renderer draws are the about's labels, not the window's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VisualLabel {
    pub dimension: String,
    pub scope_id: String,
    pub value: String,
    pub in_range: usize,
    pub entries: usize,
    pub last_observed_at: Option<String>,
}

impl VisualLabel {
    /// Every label the bundle's `contains_entry` edges name, most used
    /// first, then by key and value, so the order is the same on every run.
    /// `in_range` is zero until the projection counts its own entries.
    pub fn catalogue(bundle: &KmpBundle) -> Vec<Self> {
        let mut uses = BTreeMap::<(String, String), LabelUse>::new();
        for relationship in bundle
            .relationships()
            .iter()
            .filter(|relationship| relationship.relationship_type() == "contains_entry")
        {
            let explanation = relationship.explanation();
            let (Some(dimension), Some(scope_id)) = (
                explanation
                    .dimension()
                    .map(str::trim)
                    .filter(|key| !key.is_empty()),
                explanation
                    .scope_id()
                    .map(str::trim)
                    .filter(|scope| !scope.is_empty()),
            ) else {
                continue;
            };
            let observed = explanation
                .observed_at()
                .or(explanation.occurred_at())
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let label = uses
                .entry((dimension.to_string(), scope_id.to_string()))
                .or_default();
            label
                .entries
                .insert(relationship.target_node_id().to_string());
            if let Some(observed) = observed
                && label
                    .last_observed_at
                    .as_deref()
                    .is_none_or(|current| observed > current)
            {
                label.last_observed_at = Some(observed.to_string());
            }
        }
        let mut labels = uses
            .into_iter()
            .map(|((dimension, scope_id), label)| Self {
                value: bare_label_value(&scope_id),
                dimension,
                scope_id,
                in_range: 0,
                entries: label.entries.len(),
                last_observed_at: label.last_observed_at,
            })
            .collect::<Vec<_>>();
        Self::sort(&mut labels);
        labels
    }

    /// The same catalogue with the projected range counted in: every label a
    /// positioned entry stands in gains its `in_range`, and a label the
    /// entries name that the catalogue somehow lacks is added rather than
    /// dropped, so a row is never drawn for a label the list does not hold.
    pub fn counted<'a>(
        mut labels: Vec<Self>,
        in_range: impl IntoIterator<Item = &'a (String, String)>,
    ) -> Vec<Self> {
        let mut counts = BTreeMap::<(String, String), usize>::new();
        for (dimension, scope_id) in in_range {
            *counts
                .entry((dimension.clone(), scope_id.clone()))
                .or_default() += 1;
        }
        for label in &mut labels {
            label.in_range = counts
                .remove(&(label.dimension.clone(), label.scope_id.clone()))
                .unwrap_or_default();
        }
        for ((dimension, scope_id), count) in counts {
            labels.push(Self {
                value: bare_label_value(&scope_id),
                dimension,
                scope_id,
                in_range: count,
                entries: count,
                last_observed_at: None,
            });
        }
        Self::sort(&mut labels);
        labels
    }

    fn sort(labels: &mut [Self]) {
        labels.sort_by(|left, right| {
            right
                .entries
                .cmp(&left.entries)
                .then_with(|| left.dimension.cmp(&right.dimension))
                .then_with(|| left.value.cmp(&right.value))
        });
    }
}

#[derive(Default)]
struct LabelUse {
    entries: BTreeSet<String>,
    last_observed_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use kmp_domain::{
        BundleMetadata, BundleNode, BundleRelationship, CaseId, RelationExplanation,
        RelationSemanticClass, Role,
    };

    use super::*;

    fn node(id: &str, kind: &str) -> BundleNode {
        BundleNode::new(
            id,
            kind,
            id,
            "fixture",
            "ACTIVE",
            Vec::new(),
            BTreeMap::new(),
        )
    }

    fn contains(
        dimension: &str,
        scope_id: &str,
        entry: &str,
        observed_at: &str,
    ) -> BundleRelationship {
        BundleRelationship::new(
            scope_id,
            entry,
            "contains_entry",
            RelationExplanation::new(RelationSemanticClass::Structural)
                .with_dimension(dimension)
                .with_scope_id(scope_id)
                .with_observed_at(observed_at),
        )
    }

    fn bundle(relationships: Vec<BundleRelationship>) -> KmpBundle {
        let mut ids = BTreeSet::new();
        for relationship in &relationships {
            ids.insert(relationship.source_node_id().to_string());
            ids.insert(relationship.target_node_id().to_string());
        }
        KmpBundle::new(
            CaseId::new("about:a").expect("about"),
            Role::new("memory").expect("role"),
            node("about:a", "memory_anchor"),
            ids.into_iter()
                .map(|id| {
                    node(
                        &id,
                        if id.contains(":dimension:") {
                            "memory_dimension"
                        } else {
                            "decision"
                        },
                    )
                })
                .collect(),
            relationships,
            Vec::new(),
            BundleMetadata::initial("test"),
        )
        .expect("bundle")
    }

    const TASK_A: &str = "about:a:dimension:task-a";
    const TASK_B: &str = "about:a:dimension:task-b";
    const PROCESS: &str = "about:a:dimension:p-1";

    #[test]
    fn the_catalogue_names_every_pair_most_used_first_with_bare_values() {
        let labels = VisualLabel::catalogue(&bundle(vec![
            contains("task", TASK_A, "e-1", "2026-09-01T00:00:00Z"),
            contains("task", TASK_A, "e-2", "2026-09-03T00:00:00Z"),
            contains("task", TASK_B, "e-3", "2026-09-02T00:00:00Z"),
            contains("agentic_process", PROCESS, "e-1", "2026-09-01T00:00:00Z"),
            contains("agentic_process", PROCESS, "e-2", "2026-09-03T00:00:00Z"),
            contains("agentic_process", PROCESS, "e-3", "2026-09-02T00:00:00Z"),
        ]));
        let summary = labels
            .iter()
            .map(|label| format!("{}={} {}", label.dimension, label.value, label.entries))
            .collect::<Vec<_>>();
        assert_eq!(
            summary,
            vec!["agentic_process=p-1 3", "task=task-a 2", "task=task-b 1"]
        );
        assert_eq!(
            labels[1].scope_id, TASK_A,
            "the namespaced id stays beside the bare value"
        );
        assert_eq!(
            labels[1].last_observed_at.as_deref(),
            Some("2026-09-03T00:00:00Z")
        );
        assert!(labels.iter().all(|label| label.in_range == 0));
    }

    #[test]
    fn counting_the_range_keeps_empty_labels_and_adds_unlisted_ones() {
        let catalogue = VisualLabel::catalogue(&bundle(vec![
            contains("task", TASK_A, "e-1", "2026-09-01T00:00:00Z"),
            contains("task", TASK_B, "e-3", "2026-09-02T00:00:00Z"),
        ]));
        let in_range = [
            ("task".to_string(), TASK_A.to_string()),
            ("task".to_string(), TASK_A.to_string()),
            (
                "incident".to_string(),
                "about:a:dimension:inc-9".to_string(),
            ),
        ];
        let counted = VisualLabel::counted(catalogue, in_range.iter());
        let by_value = counted
            .iter()
            .map(|label| (label.value.as_str(), (label.in_range, label.entries)))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(by_value["task-a"], (2, 1));
        assert_eq!(by_value["task-b"], (0, 1), "empty here, still a row");
        assert_eq!(by_value["inc-9"], (1, 1), "never a row without a label");
    }
}
