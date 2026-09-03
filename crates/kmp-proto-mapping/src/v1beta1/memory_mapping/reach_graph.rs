use std::collections::{BTreeMap, BTreeSet};

use kmp_domain::{KmpBundle, RelationSemanticClass, RelationSignal};

use super::relation_reach::RelationReach;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReachEdge {
    target: String,
    relation: String,
    weight: u32,
}

/// The subgraph retrieval is allowed to walk.
///
/// Two filters decide membership, and both are narrow on purpose.
///
/// The relation must say what led to what: only causal and motivational
/// classes are walkable. An evidential comparison — `checked_against`
/// between two items someone triaged in the same sitting — is a real,
/// proven relation and still not a route to an answer, because being
/// compared with something is not being part of it.
///
/// And it must carry its own proof. An anemic succession edge, a structural
/// containment edge and an invented relation type are all absent, so a
/// high-degree node cannot drag its neighbourhood along with it.
///
/// A second, separate subgraph holds declared equivalence. `restates`,
/// `same_event_as` and `same_entity_as` do not say what led to what; they
/// say two memories are the same thing in other words, and a writer proved
/// it. A restatement of what the question matched is what the question
/// matched — which is the one route to a paraphrase that needs no table and
/// no model, only that somebody wrote it down.
#[derive(Debug, Default)]
pub(super) struct ReachGraph {
    edges: BTreeMap<String, Vec<ReachEdge>>,
    equivalences: BTreeMap<String, Vec<ReachEdge>>,
}

/// The relation types that declare two memories to be the same thing.
///
/// `supersedes` and `corrects` are deliberately absent: they declare a
/// replacement, and the lifecycle already decides which side of one is
/// current. `contradicts` keeps two claims in tension, which is the opposite
/// of the same thing.
const EQUIVALENCE_RELATIONS: &[&str] = &["restates", "same_event_as", "same_entity_as"];

impl ReachGraph {
    pub(super) fn from_bundle(bundle: &KmpBundle) -> Self {
        let mut edges = BTreeMap::<String, Vec<ReachEdge>>::new();
        let mut equivalences = BTreeMap::<String, Vec<ReachEdge>>::new();

        for relationship in bundle.relationships() {
            let explanation = relationship.explanation();
            let relation = relationship.relationship_type();
            let signal = RelationSignal::read(relation, explanation);
            if !signal.carries_retrieval() {
                continue;
            }
            let weight = signal.weight();

            // Equivalence is read whatever its class, because it is not a
            // claim about causation; it still has to carry its own proof.
            if EQUIVALENCE_RELATIONS.contains(&relation) {
                insert_both_ways(
                    &mut equivalences,
                    relationship.source_node_id(),
                    relationship.target_node_id(),
                    relation,
                    weight,
                );
                continue;
            }
            if !is_walkable_class(explanation.semantic_class()) {
                continue;
            }

            insert_both_ways(
                &mut edges,
                relationship.source_node_id(),
                relationship.target_node_id(),
                relation,
                weight,
            );

            // A declared causal parent is a one-to-one pointer the writer
            // stored deliberately, so it is walkable. `decision_id` is not:
            // it groups everything that hung off one decision, and following
            // it would reintroduce exactly the fan-out this graph excludes.
            if let Some(parent) = explanation.caused_by_node_id() {
                insert_both_ways(
                    &mut edges,
                    relationship.source_node_id(),
                    parent,
                    "caused_by",
                    weight,
                );
            }
        }

        for adjacency in edges.values_mut().chain(equivalences.values_mut()) {
            adjacency.sort_by(|left, right| {
                right
                    .weight
                    .cmp(&left.weight)
                    .then_with(|| left.target.cmp(&right.target))
                    .then_with(|| left.relation.cmp(&right.relation))
            });
            adjacency.dedup();
        }

        Self {
            edges,
            equivalences,
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    pub(super) fn has_equivalences(&self) -> bool {
        !self.equivalences.is_empty()
    }

    /// The memories a writer declared to be the same thing as any of the
    /// seeds, one hop away and never further: a restatement of a
    /// restatement is a chain nobody vouched for as a whole.
    ///
    /// Each equivalent is reported once, from the strongest edge that
    /// reaches it, and the seeds themselves and everything `blocked` stay
    /// out — a superseded claim is not rescued by having been restated.
    pub(super) fn equivalents_of(
        &self,
        seeds: &BTreeSet<String>,
        blocked: &BTreeSet<String>,
    ) -> BTreeMap<String, RelationReach> {
        let mut equivalents = BTreeMap::<String, RelationReach>::new();
        for seed in seeds {
            for edge in self.equivalences.get(seed).into_iter().flatten() {
                if seeds.contains(&edge.target) || blocked.contains(&edge.target) {
                    continue;
                }
                let stronger = equivalents
                    .get(&edge.target)
                    .is_none_or(|known| edge.weight > known.weight);
                if stronger {
                    equivalents.insert(
                        edge.target.clone(),
                        RelationReach {
                            hops: 1,
                            weight: edge.weight,
                            from_ref: seed.clone(),
                            via_relation: edge.relation.clone(),
                        },
                    );
                }
            }
        }
        equivalents
    }

    /// Walks out from what the question already matched.
    ///
    /// Seeds are candidates that earned their place on their own text, so
    /// noise cannot seed a walk. `blocked` carries what must never be
    /// rescued — a superseded claim stays superseded however strong the edge
    /// that points at it.
    ///
    /// A path is only as strong as its weakest edge, and results are ordered
    /// by hop first, so nearer and better-proven memories win the budget.
    pub(super) fn reach_from(
        &self,
        seeds: &BTreeSet<String>,
        blocked: &BTreeSet<String>,
        max_hops: u32,
        max_reached: usize,
    ) -> BTreeMap<String, RelationReach> {
        let mut reached = BTreeMap::new();
        if self.edges.is_empty() || seeds.is_empty() || max_reached == 0 {
            return reached;
        }

        let mut visited = seeds.clone();
        let mut frontier = seeds
            .iter()
            .map(|seed| (seed.clone(), u32::MAX))
            .collect::<Vec<_>>();

        for hop in 1..=max_hops {
            let mut next = BTreeMap::<String, (u32, String, String)>::new();
            for (node, carried) in &frontier {
                for edge in self.edges.get(node).into_iter().flatten() {
                    if visited.contains(&edge.target) || blocked.contains(&edge.target) {
                        continue;
                    }
                    let weight = (*carried).min(edge.weight);
                    let improves = next
                        .get(&edge.target)
                        .is_none_or(|(best, _, _)| weight > *best);
                    if improves {
                        next.insert(
                            edge.target.clone(),
                            (weight, node.clone(), edge.relation.clone()),
                        );
                    }
                }
            }
            if next.is_empty() {
                break;
            }

            let mut ordered = next.into_iter().collect::<Vec<_>>();
            ordered
                .sort_by(|left, right| right.1.0.cmp(&left.1.0).then_with(|| left.0.cmp(&right.0)));

            let mut new_frontier = Vec::new();
            for (target, (weight, from_ref, via_relation)) in ordered {
                if reached.len() >= max_reached {
                    break;
                }
                visited.insert(target.clone());
                new_frontier.push((target.clone(), weight));
                reached.insert(
                    target,
                    RelationReach {
                        hops: hop,
                        weight,
                        from_ref,
                        via_relation,
                    },
                );
            }
            if reached.len() >= max_reached || new_frontier.is_empty() {
                break;
            }
            frontier = new_frontier;
        }

        reached
    }
}

/// The classes that answer *what led to this*. Evidential, constraint,
/// procedural and structural relations still weigh and reorder candidates the
/// question reached on its own; they do not carry retrieval to new ones.
fn is_walkable_class(class: &RelationSemanticClass) -> bool {
    matches!(
        class,
        RelationSemanticClass::Causal | RelationSemanticClass::Motivational
    )
}

fn insert_both_ways(
    edges: &mut BTreeMap<String, Vec<ReachEdge>>,
    source: &str,
    target: &str,
    relation: &str,
    weight: u32,
) {
    if source == target {
        return;
    }
    edges
        .entry(source.to_string())
        .or_default()
        .push(ReachEdge {
            target: target.to_string(),
            relation: relation.to_string(),
            weight,
        });
    edges
        .entry(target.to_string())
        .or_default()
        .push(ReachEdge {
            target: source.to_string(),
            relation: relation.to_string(),
            weight,
        });
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap as StdMap;

    use kmp_domain::{
        BundleMetadata, BundleNode, BundleRelationship, CaseId, RelationExplanation,
        RelationSemanticClass, Role,
    };

    use super::*;

    fn node(id: &str) -> BundleNode {
        BundleNode::new(
            id,
            "memory",
            id,
            "fixture",
            "ACTIVE",
            Vec::new(),
            StdMap::new(),
        )
    }

    fn proven(class: RelationSemanticClass) -> RelationExplanation {
        RelationExplanation::new(class)
            .with_rationale("the deploy failed because the gate had never run")
            .with_evidence("workflow log 4711")
            .with_confidence("high")
    }

    fn bundle(relationships: Vec<BundleRelationship>, neighbours: &[&str]) -> KmpBundle {
        KmpBundle::new(
            CaseId::new("claim:root").expect("valid case id"),
            Role::new("answerer").expect("valid role"),
            node("claim:root"),
            neighbours.iter().map(|id| node(id)).collect(),
            relationships,
            Vec::new(),
            BundleMetadata::initial("test"),
        )
        .expect("valid bundle")
    }

    #[test]
    fn a_proven_causal_edge_reaches_upstream_memory() {
        let graph = ReachGraph::from_bundle(&bundle(
            vec![BundleRelationship::new(
                "claim:symptom",
                "claim:root-cause",
                "triggers",
                proven(RelationSemanticClass::Causal),
            )],
            &["claim:symptom", "claim:root-cause"],
        ));

        let reached = graph.reach_from(
            &BTreeSet::from(["claim:symptom".to_string()]),
            &BTreeSet::new(),
            2,
            8,
        );

        let hop = reached.get("claim:root-cause").expect("upstream reached");
        assert_eq!(hop.hops, 1);
        assert_eq!(hop.from_ref, "claim:symptom");
        assert_eq!(hop.via_relation, "triggers");
    }

    #[test]
    fn a_proven_evidential_comparison_is_not_a_route_to_an_answer() {
        let graph = ReachGraph::from_bundle(&bundle(
            vec![BundleRelationship::new(
                "claim:constraint",
                "claim:unrelated-race",
                "checked_against",
                proven(RelationSemanticClass::Evidential),
            )],
            &["claim:constraint", "claim:unrelated-race"],
        ));

        assert!(graph.is_empty());
    }

    #[test]
    fn an_anemic_edge_is_not_walkable() {
        let graph = ReachGraph::from_bundle(&bundle(
            vec![BundleRelationship::new(
                "claim:first",
                "claim:second",
                "follows",
                proven(RelationSemanticClass::Procedural),
            )],
            &["claim:first", "claim:second"],
        ));

        assert!(graph.is_empty());
        assert!(
            graph
                .reach_from(
                    &BTreeSet::from(["claim:first".to_string()]),
                    &BTreeSet::new(),
                    2,
                    8
                )
                .is_empty()
        );
    }

    #[test]
    fn a_blocked_reference_is_never_rescued() {
        let graph = ReachGraph::from_bundle(&bundle(
            vec![BundleRelationship::new(
                "claim:new",
                "claim:old",
                "triggers",
                proven(RelationSemanticClass::Causal),
            )],
            &["claim:new", "claim:old"],
        ));

        let reached = graph.reach_from(
            &BTreeSet::from(["claim:new".to_string()]),
            &BTreeSet::from(["claim:old".to_string()]),
            2,
            8,
        );

        assert!(reached.is_empty());
    }

    #[test]
    fn a_declared_causal_parent_is_walkable_without_its_own_edge() {
        let graph = ReachGraph::from_bundle(&bundle(
            vec![BundleRelationship::new(
                "claim:symptom",
                "claim:report",
                "depends_on",
                proven(RelationSemanticClass::Causal).with_caused_by_node_id("claim:origin"),
            )],
            &["claim:symptom", "claim:report", "claim:origin"],
        ));

        let reached = graph.reach_from(
            &BTreeSet::from(["claim:symptom".to_string()]),
            &BTreeSet::new(),
            1,
            8,
        );

        assert_eq!(reached["claim:origin"].via_relation, "caused_by");
    }

    #[test]
    fn the_walk_stops_at_the_hop_and_result_budget() {
        let graph = ReachGraph::from_bundle(&bundle(
            vec![
                BundleRelationship::new(
                    "claim:a",
                    "claim:b",
                    "triggers",
                    proven(RelationSemanticClass::Causal),
                ),
                BundleRelationship::new(
                    "claim:b",
                    "claim:c",
                    "triggers",
                    proven(RelationSemanticClass::Causal),
                ),
                BundleRelationship::new(
                    "claim:c",
                    "claim:d",
                    "triggers",
                    proven(RelationSemanticClass::Causal),
                ),
            ],
            &["claim:a", "claim:b", "claim:c", "claim:d"],
        ));
        let seeds = BTreeSet::from(["claim:a".to_string()]);

        let two_hops = graph.reach_from(&seeds, &BTreeSet::new(), 2, 8);
        assert_eq!(two_hops.len(), 2);
        assert_eq!(two_hops["claim:c"].hops, 2);
        assert!(!two_hops.contains_key("claim:d"));

        let budgeted = graph.reach_from(&seeds, &BTreeSet::new(), 3, 1);
        assert_eq!(budgeted.len(), 1);
        assert!(budgeted.contains_key("claim:b"));
    }

    #[test]
    fn a_path_is_only_as_strong_as_its_weakest_edge() {
        let graph = ReachGraph::from_bundle(&bundle(
            vec![
                BundleRelationship::new(
                    "claim:a",
                    "claim:b",
                    "triggers",
                    proven(RelationSemanticClass::Causal),
                ),
                BundleRelationship::new(
                    "claim:b",
                    "claim:c",
                    "chosen_because",
                    proven(RelationSemanticClass::Motivational),
                ),
            ],
            &["claim:a", "claim:b", "claim:c"],
        ));

        let reached = graph.reach_from(
            &BTreeSet::from(["claim:a".to_string()]),
            &BTreeSet::new(),
            2,
            8,
        );

        assert!(reached["claim:c"].weight < reached["claim:b"].weight);
    }

    #[test]
    fn a_proven_restatement_is_an_equivalent_and_not_a_causal_route() {
        let graph = ReachGraph::from_bundle(&bundle(
            vec![BundleRelationship::new(
                "claim:in-other-words",
                "claim:original",
                "restates",
                proven(RelationSemanticClass::Evidential),
            )],
            &["claim:in-other-words", "claim:original"],
        ));

        assert!(graph.is_empty(), "equivalence is not a walk");
        assert!(graph.has_equivalences());
        let equivalents = graph.equivalents_of(
            &BTreeSet::from(["claim:original".to_string()]),
            &BTreeSet::new(),
        );
        let hop = equivalents
            .get("claim:in-other-words")
            .expect("the restatement is reached from either side");
        assert_eq!(hop.hops, 1);
        assert_eq!(hop.from_ref, "claim:original");
        assert_eq!(hop.via_relation, "restates");
    }

    #[test]
    fn equivalence_stops_at_one_hop_and_respects_the_block_list() {
        let graph = ReachGraph::from_bundle(&bundle(
            vec![
                BundleRelationship::new(
                    "claim:a",
                    "claim:b",
                    "same_event_as",
                    proven(RelationSemanticClass::Evidential),
                ),
                BundleRelationship::new(
                    "claim:b",
                    "claim:c",
                    "same_event_as",
                    proven(RelationSemanticClass::Evidential),
                ),
                BundleRelationship::new(
                    "claim:a",
                    "claim:superseded",
                    "restates",
                    proven(RelationSemanticClass::Evidential),
                ),
            ],
            &["claim:a", "claim:b", "claim:c", "claim:superseded"],
        ));

        let equivalents = graph.equivalents_of(
            &BTreeSet::from(["claim:a".to_string()]),
            &BTreeSet::from(["claim:superseded".to_string()]),
        );

        assert!(equivalents.contains_key("claim:b"));
        assert!(
            !equivalents.contains_key("claim:c"),
            "a chain nobody vouched for as a whole"
        );
        assert!(!equivalents.contains_key("claim:superseded"));
    }

    #[test]
    fn a_restatement_without_proof_is_not_an_equivalent() {
        let graph = ReachGraph::from_bundle(&bundle(
            vec![BundleRelationship::new(
                "claim:in-other-words",
                "claim:original",
                "restates",
                RelationExplanation::new(RelationSemanticClass::Evidential),
            )],
            &["claim:in-other-words", "claim:original"],
        ));

        assert!(!graph.has_equivalences());
    }

    #[test]
    fn a_replacement_is_not_an_equivalence() {
        let graph = ReachGraph::from_bundle(&bundle(
            vec![BundleRelationship::new(
                "claim:new",
                "claim:old",
                "supersedes",
                proven(RelationSemanticClass::Evidential),
            )],
            &["claim:new", "claim:old"],
        ));

        assert!(!graph.has_equivalences());
    }
}
