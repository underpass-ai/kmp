use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::BundleRelationship;

/// Returns the shortest directed relationship path from `from` to `to`.
///
/// A trace is proof of reachability, not the connected neighborhood around a
/// ref. `None` therefore means the target was not reached; callers must not
/// turn the explored edges into an answer. The relationship slice may be in
/// storage order — the returned path is always walk order.
pub fn directed_relationship_path<'a>(
    relationships: &'a [BundleRelationship],
    from: &str,
    to: &str,
) -> Option<Vec<&'a BundleRelationship>> {
    if from == to {
        return Some(Vec::new());
    }

    let mut queue = VecDeque::from([from.to_string()]);
    let mut seen = BTreeSet::from([from.to_string()]);
    let mut parent = BTreeMap::<String, (String, usize)>::new();

    while let Some(current) = queue.pop_front() {
        for (index, relationship) in relationships
            .iter()
            .enumerate()
            .filter(|(_, edge)| edge.source_node_id() == current)
        {
            let next = relationship.target_node_id();
            if !seen.insert(next.to_string()) {
                continue;
            }
            parent.insert(next.to_string(), (current.clone(), index));
            if next == to {
                let mut indices = Vec::new();
                let mut cursor = to;
                while cursor != from {
                    let (previous, edge_index) = parent.get(cursor)?;
                    indices.push(*edge_index);
                    cursor = previous;
                }
                indices.reverse();
                return Some(
                    indices
                        .into_iter()
                        .map(|edge_index| &relationships[edge_index])
                        .collect(),
                );
            }
            queue.push_back(next.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::{BundleRelationship, RelationExplanation, RelationSemanticClass};

    use super::directed_relationship_path;

    fn edge(from: &str, to: &str) -> BundleRelationship {
        BundleRelationship::new(
            from,
            to,
            "triggers",
            RelationExplanation::new(RelationSemanticClass::Causal),
        )
    }

    #[test]
    fn finds_one_ordered_path_without_appending_the_neighborhood() {
        let relationships = vec![
            edge("c2", "c1"),
            edge("c3", "c2"),
            edge("c1", "old"),
            edge("c5", "c4"),
            edge("c4", "c3"),
        ];
        let path = directed_relationship_path(&relationships, "c5", "c1").expect("path");
        assert_eq!(
            path.iter()
                .map(|edge| (edge.source_node_id(), edge.target_node_id()))
                .collect::<Vec<_>>(),
            vec![("c5", "c4"), ("c4", "c3"), ("c3", "c2"), ("c2", "c1")]
        );
    }

    #[test]
    fn refuses_to_present_reverse_or_explored_edges_as_a_path() {
        let relationships = vec![edge("c2", "c1"), edge("c1", "old")];
        assert!(directed_relationship_path(&relationships, "c1", "c2").is_none());
    }
}
