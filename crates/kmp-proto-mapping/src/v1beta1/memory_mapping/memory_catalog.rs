use std::collections::{BTreeMap, BTreeSet};

use kmp_domain::{KmpBundle, MemoryDimensionIdentity};
use kmp_proto::v1beta1::MemoryLabel;
use prost_types::Timestamp;

use super::scalars::timestamp_from_sort_or_rfc3339;

/// The catalogue the abouts of a bundle carry: every (key, value) pair their
/// entries stand in, read off the `contains_entry` edges.
///
/// A coordinate names a `dimension` kind and a `scope_id`. Read as a label,
/// the kind is the key and the scope is the value, and the store already
/// holds one node per pair. What it never held was a way to read them back:
/// a writer that cannot see the catalogue names a new label where one
/// exists, and a filter that cannot see it is a guess, because a selection
/// does not demote what it misses — it hides it.
///
/// The bundle's own about comes first, then any other about the wake read,
/// by name. Inside an about, the label most entries stand in comes first,
/// then key, then value, so the order is the same on every run.
pub(super) fn labels_from_bundle(bundle: &KmpBundle) -> Vec<MemoryLabel> {
    let current_about = bundle.root_node().node_id();
    let mut uses = BTreeMap::<(String, String, String), LabelUse>::new();
    for relationship in bundle
        .relationships()
        .iter()
        .filter(|relationship| relationship.relationship_type() == "contains_entry")
    {
        let explanation = relationship.explanation();
        let Some(key) = explanation
            .dimension()
            .map(str::trim)
            .filter(|key| !key.is_empty())
        else {
            continue;
        };
        let source = MemoryDimensionIdentity::parse(relationship.source_node_id());
        let about = source
            .as_ref()
            .map(|identity| identity.about().to_string())
            .unwrap_or_else(|| current_about.to_string());
        let value = explanation
            .scope_id()
            .map(bare_value)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                source
                    .as_ref()
                    .map(|identity| identity.dimension_id().to_string())
            });
        let Some(value) = value else {
            continue;
        };
        let observed =
            timestamp_from_sort_or_rfc3339(explanation.observed_at().or(explanation.occurred_at()));
        let label = uses.entry((about, key.to_string(), value)).or_default();
        label
            .entries
            .insert(relationship.target_node_id().to_string());
        if let Some(observed) = observed
            && label
                .last_observed_at
                .as_ref()
                .is_none_or(|current| instant(&observed) > instant(current))
        {
            label.last_observed_at = Some(observed);
        }
    }

    let mut labels = uses
        .into_iter()
        .map(|((about, key, value), label)| MemoryLabel {
            about,
            key,
            value,
            entries: u32::try_from(label.entries.len()).unwrap_or(u32::MAX),
            last_observed_at: label.last_observed_at,
        })
        .collect::<Vec<_>>();
    labels.sort_by(|left, right| {
        (left.about != current_about, &left.about)
            .cmp(&(right.about != current_about, &right.about))
            .then_with(|| right.entries.cmp(&left.entries))
            .then_with(|| left.key.cmp(&right.key))
            .then_with(|| left.value.cmp(&right.value))
    });
    labels
}

#[derive(Default)]
struct LabelUse {
    entries: BTreeSet<String>,
    last_observed_at: Option<Timestamp>,
}

/// A scope id as the writer named it: reads hand out the namespaced form,
/// and the catalogue speaks in the writer's words.
fn bare_value(scope_id: &str) -> String {
    MemoryDimensionIdentity::parse(scope_id)
        .map(|identity| identity.dimension_id().to_string())
        .unwrap_or_else(|| scope_id.trim().to_string())
}

fn instant(value: &Timestamp) -> (i64, i32) {
    (value.seconds, value.nanos)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap as StdMap;

    use kmp_domain::{
        BundleMetadata, BundleNode, BundleRelationship, CaseId, RelationExplanation,
        RelationSemanticClass, Role,
    };

    use super::*;

    fn node(id: &str, kind: &str) -> BundleNode {
        BundleNode::new(id, kind, id, "fixture", "ACTIVE", Vec::new(), StdMap::new())
    }

    fn coordinate(
        dimension_node: &str,
        key: &str,
        value: &str,
        entry: &str,
        observed_at: &str,
    ) -> BundleRelationship {
        let explanation = RelationExplanation::new(RelationSemanticClass::Structural)
            .with_dimension(key)
            .with_scope_id(value)
            .with_observed_at(observed_at);
        BundleRelationship::new(dimension_node, entry, "contains_entry", explanation)
    }

    fn bundle(about: &str, relationships: Vec<BundleRelationship>) -> KmpBundle {
        let mut ids = BTreeSet::new();
        for relationship in &relationships {
            ids.insert(relationship.source_node_id().to_string());
            ids.insert(relationship.target_node_id().to_string());
        }
        KmpBundle::new(
            CaseId::new(about).expect("case id"),
            Role::new("resumer").expect("role"),
            node(about, "memory_about"),
            ids.iter()
                .map(|id| {
                    node(
                        id,
                        if id.contains(":dimension:") {
                            "memory_dimension"
                        } else {
                            "memory"
                        },
                    )
                })
                .collect(),
            relationships,
            Vec::new(),
            BundleMetadata::initial("test"),
        )
        .expect("valid bundle")
    }

    #[test]
    fn the_catalogue_counts_distinct_entries_per_label_and_keeps_the_latest_instant() {
        let labels = labels_from_bundle(&bundle(
            "project:x",
            vec![
                coordinate(
                    "about:project:x:dimension:kmp-506",
                    "task",
                    "kmp-506",
                    "project:x:entry:a",
                    "2026-09-01T10:00:00Z",
                ),
                coordinate(
                    "about:project:x:dimension:kmp-506",
                    "task",
                    "kmp-506",
                    "project:x:entry:b",
                    "2026-09-03T10:00:00Z",
                ),
                coordinate(
                    "about:project:x:dimension:kmp-506",
                    "task",
                    "kmp-506",
                    "project:x:entry:b",
                    "2026-09-02T10:00:00Z",
                ),
                coordinate(
                    "about:project:x:dimension:diagnostics",
                    "agentic_process",
                    "diagnostics",
                    "project:x:entry:a",
                    "2026-09-04T10:00:00Z",
                ),
            ],
        ));

        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0].key, "task");
        assert_eq!(labels[0].value, "kmp-506");
        assert_eq!(labels[0].about, "project:x");
        assert_eq!(labels[0].entries, 2, "the same entry counts once per label");
        assert_eq!(
            labels[0].last_observed_at.as_ref().map(ToString::to_string),
            Some("2026-09-03T10:00:00Z".to_string())
        );
        assert_eq!(labels[1].key, "agentic_process");
        assert_eq!(labels[1].entries, 1);
    }

    #[test]
    fn the_current_about_comes_first_and_ties_break_by_key_then_value() {
        let labels = labels_from_bundle(&bundle(
            "project:x",
            vec![
                coordinate(
                    "about:project:y:dimension:kmp-506",
                    "task",
                    "kmp-506",
                    "project:y:entry:c",
                    "2026-09-05T10:00:00Z",
                ),
                coordinate(
                    "about:project:x:dimension:why",
                    "agentic_episode",
                    "why",
                    "project:x:entry:a",
                    "2026-09-01T10:00:00Z",
                ),
                coordinate(
                    "about:project:x:dimension:kmp-506",
                    "task",
                    "kmp-506",
                    "project:x:entry:b",
                    "2026-09-01T10:00:00Z",
                ),
            ],
        ));

        let order = labels
            .iter()
            .map(|label| format!("{} {}={}", label.about, label.key, label.value))
            .collect::<Vec<_>>();
        assert_eq!(
            order,
            vec![
                "project:x agentic_episode=why",
                "project:x task=kmp-506",
                "project:y task=kmp-506",
            ]
        );
    }

    #[test]
    fn a_namespaced_scope_reads_back_in_the_writers_words() {
        let labels = labels_from_bundle(&bundle(
            "project:x",
            vec![coordinate(
                "about:project:x:dimension:kmp-506",
                "task",
                "about:project:x:dimension:kmp-506",
                "project:x:entry:a",
                "2026-09-01T10:00:00Z",
            )],
        ));

        assert_eq!(labels[0].value, "kmp-506");
    }

    #[test]
    fn an_edge_without_a_dimension_kind_is_not_a_label() {
        let explanation = RelationExplanation::new(RelationSemanticClass::Structural)
            .with_scope_id("kmp-506")
            .with_observed_at("2026-09-01T10:00:00Z");
        let labels = labels_from_bundle(&bundle(
            "project:x",
            vec![BundleRelationship::new(
                "about:project:x:dimension:kmp-506",
                "project:x:entry:a",
                "contains_entry",
                explanation,
            )],
        ));

        assert!(labels.is_empty());
    }
}
