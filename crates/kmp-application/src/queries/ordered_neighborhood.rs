use std::cmp::Ordering;

use kmp_domain::{NodeNeighborhood, NodeProjection, NodeRelationProjection};

pub(crate) fn ordered_neighborhood(mut neighborhood: NodeNeighborhood) -> NodeNeighborhood {
    neighborhood.neighbors.sort_by(compare_nodes);
    neighborhood.relations.sort_by(compare_relations);
    neighborhood
}

fn compare_nodes(left: &NodeProjection, right: &NodeProjection) -> Ordering {
    (&left.node_id, &left.node_kind, &left.title).cmp(&(
        &right.node_id,
        &right.node_kind,
        &right.title,
    ))
}

fn compare_relations(left: &NodeRelationProjection, right: &NodeRelationProjection) -> Ordering {
    left.explanation
        .sequence()
        .unwrap_or(u32::MAX)
        .cmp(&right.explanation.sequence().unwrap_or(u32::MAX))
        .then_with(|| {
            (
                &left.source_node_id,
                &left.target_node_id,
                &left.relation_type,
            )
                .cmp(&(
                    &right.source_node_id,
                    &right.target_node_id,
                    &right.relation_type,
                ))
        })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use kmp_domain::{
        NodeNeighborhood, NodeProjection, NodeRelationProjection, RelationExplanation,
        RelationSemanticClass,
    };

    use super::ordered_neighborhood;

    #[test]
    fn ordered_neighborhood_sorts_neighbors_and_relationships() {
        let ordered = ordered_neighborhood(NodeNeighborhood {
            root: node("story-123", "story", "Root"),
            neighbors: vec![
                node("task-2", "task", "Task B"),
                node("decision-1", "decision", "Decision"),
                node("task-1", "task", "Task A"),
            ],
            relations: vec![
                relation("story-123", "task-2", "HAS_TASK"),
                relation("story-123", "decision-1", "RECORDS"),
                relation("story-123", "task-1", "HAS_TASK"),
            ],
        });

        assert_eq!(ordered.neighbors[0].node_id, "decision-1");
        assert_eq!(ordered.neighbors[1].node_id, "task-1");
        assert_eq!(ordered.neighbors[2].node_id, "task-2");
        assert_eq!(ordered.relations[0].target_node_id, "decision-1");
        assert_eq!(ordered.relations[1].target_node_id, "task-1");
        assert_eq!(ordered.relations[2].target_node_id, "task-2");
    }

    #[test]
    fn ordered_neighborhood_prefers_relation_sequence_when_present() {
        let ordered = ordered_neighborhood(NodeNeighborhood {
            root: node("story-123", "story", "Root"),
            neighbors: vec![
                node("task-1", "task", "Task"),
                node("task-2", "task", "Task"),
            ],
            relations: vec![
                relation_with_properties("story-123", "task-2", "HAS_TASK", [("sequence", "2")]),
                relation_with_properties("story-123", "task-1", "HAS_TASK", [("sequence", "1")]),
            ],
        });

        assert_eq!(ordered.relations[0].target_node_id, "task-1");
        assert_eq!(ordered.relations[1].target_node_id, "task-2");
    }

    fn node(node_id: &str, node_kind: &str, title: &str) -> NodeProjection {
        NodeProjection {
            node_id: node_id.to_string(),
            node_kind: node_kind.to_string(),
            title: title.to_string(),
            summary: String::new(),
            status: "ACTIVE".to_string(),
            labels: Vec::new(),
            properties: BTreeMap::new(),
            provenance: None,
        }
    }

    fn relation(
        source_node_id: &str,
        target_node_id: &str,
        relation_type: &str,
    ) -> NodeRelationProjection {
        NodeRelationProjection {
            source_node_id: source_node_id.to_string(),
            target_node_id: target_node_id.to_string(),
            relation_type: relation_type.to_string(),
            explanation: RelationExplanation::new(RelationSemanticClass::Structural),
        }
    }

    fn relation_with_properties<const N: usize>(
        source_node_id: &str,
        target_node_id: &str,
        relation_type: &str,
        properties: [(&str, &str); N],
    ) -> NodeRelationProjection {
        let mut explanation = RelationExplanation::new(RelationSemanticClass::Structural);

        for (key, value) in properties {
            if key == "sequence" {
                explanation = explanation
                    .with_sequence(value.parse::<u32>().expect("sequence should parse as u32"));
            }
        }

        NodeRelationProjection {
            source_node_id: source_node_id.to_string(),
            target_node_id: target_node_id.to_string(),
            relation_type: relation_type.to_string(),
            explanation,
        }
    }
}
