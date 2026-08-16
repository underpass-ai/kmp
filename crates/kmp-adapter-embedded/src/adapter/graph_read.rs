use std::collections::{BTreeMap, BTreeSet, VecDeque};

use kmp_domain::{
    ContextPathNeighborhood, GraphNeighborhoodReader, MemoryAboutIndexReader, NodeNeighborhood,
    NodeProjection, NodeRelationProjection, NodeRelationshipReader, NodeRelationships, PortError,
};

use super::engine::{Key, ReadTx, Table};
use super::projection_write::MEMORY_ANCHOR_KIND;
use super::serdes::{NodeRecord, decode, decode_explanation};
use super::store::EmbeddedKernelStore;

fn load_node(tx: &dyn ReadTx, node_id: &str) -> Result<Option<NodeProjection>, PortError> {
    match tx.get(Table::Nodes, Key::Str(node_id))? {
        Some(raw) => Ok(Some(
            decode::<NodeRecord>("graph node", &raw)?.into_projection()?,
        )),
        None => Ok(None),
    }
}

fn outgoing_rows(tx: &dyn ReadTx, source: &str) -> Result<Vec<NodeRelationProjection>, PortError> {
    tx.scan_str3_by_first(Table::Relations, source)?
        .into_iter()
        .map(|((source_node_id, target_node_id, relation_type), raw)| {
            Ok(NodeRelationProjection {
                source_node_id,
                target_node_id,
                relation_type,
                explanation: decode_explanation(&raw)?,
            })
        })
        .collect()
}

fn outgoing_targets(tx: &dyn ReadTx, source: &str) -> Result<Vec<String>, PortError> {
    Ok(tx
        .scan_str3_by_first(Table::Relations, source)?
        .into_iter()
        .map(|((_, target, _), _)| target)
        .collect())
}

fn reachable_outward(
    tx: &dyn ReadTx,
    root_node_id: &str,
    depth: u32,
) -> Result<BTreeSet<String>, PortError> {
    let mut visited = BTreeSet::from([root_node_id.to_string()]);
    let mut reachable = BTreeSet::new();
    let mut frontier = VecDeque::from([(root_node_id.to_string(), 0u32)]);

    while let Some((node_id, hops)) = frontier.pop_front() {
        if hops == depth {
            continue;
        }
        for target in outgoing_targets(tx, &node_id)? {
            if visited.insert(target.clone()) {
                reachable.insert(target.clone());
                frontier.push_back((target, hops + 1));
            }
        }
    }

    reachable.remove(root_node_id);
    Ok(reachable)
}

fn relations_among(
    tx: &dyn ReadTx,
    selected: &BTreeSet<String>,
) -> Result<Vec<NodeRelationProjection>, PortError> {
    let mut rows = Vec::new();
    for source in selected {
        for relation in outgoing_rows(tx, source)? {
            if selected.contains(&relation.target_node_id) {
                rows.push(relation);
            }
        }
    }
    Ok(rows)
}

fn selected_projections(
    tx: &dyn ReadTx,
    selected: &BTreeSet<String>,
    root_node_id: &str,
) -> Result<Vec<NodeProjection>, PortError> {
    let mut projections = Vec::new();
    for node_id in selected {
        if node_id == root_node_id {
            continue;
        }
        if let Some(projection) = load_node(tx, node_id)? {
            projections.push(projection);
        }
    }
    Ok(projections)
}

fn shortest_outward_path(
    tx: &dyn ReadTx,
    root_node_id: &str,
    target_node_id: &str,
) -> Result<Option<Vec<String>>, PortError> {
    let mut predecessors = BTreeMap::<String, String>::new();
    let mut visited = BTreeSet::from([root_node_id.to_string()]);
    let mut frontier = VecDeque::from([root_node_id.to_string()]);

    while let Some(node_id) = frontier.pop_front() {
        for target in outgoing_targets(tx, &node_id)? {
            if !visited.insert(target.clone()) {
                continue;
            }
            predecessors.insert(target.clone(), node_id.clone());
            if target == target_node_id {
                let mut path = vec![target.clone()];
                let mut current = target.as_str();
                while let Some(previous) = predecessors.get(current) {
                    path.push(previous.clone());
                    current = previous;
                }
                path.reverse();
                return Ok(Some(path));
            }
            frontier.push_back(target);
        }
    }

    Ok(None)
}

impl GraphNeighborhoodReader for EmbeddedKernelStore {
    async fn load_neighborhood(
        &self,
        root_node_id: &str,
        depth: u32,
    ) -> Result<Option<NodeNeighborhood>, PortError> {
        let root_node_id = root_node_id.to_string();
        self.run(move |store| {
            let tx = store.begin_read()?;
            let tx = tx.as_ref();

            let Some(root) = load_node(tx, &root_node_id)? else {
                return Ok(None);
            };

            let reachable = reachable_outward(tx, &root_node_id, depth)?;
            // Mirrors the Neo4j neighborhood query: an empty neighborhood
            // reports no relations, even for self-referential root edges.
            let relation_rows = if reachable.is_empty() {
                Vec::new()
            } else {
                let mut selected = reachable.clone();
                selected.insert(root_node_id.clone());
                relations_among(tx, &selected)?
            };

            Ok(Some(NodeNeighborhood {
                neighbors: selected_projections(tx, &reachable, &root_node_id)?,
                relations: relation_rows,
                root,
            }))
        })
        .await
    }

    async fn load_context_path(
        &self,
        root_node_id: &str,
        target_node_id: &str,
        subtree_depth: u32,
    ) -> Result<Option<ContextPathNeighborhood>, PortError> {
        let root_node_id = root_node_id.to_string();
        let target_node_id = target_node_id.to_string();
        self.run(move |store| {
            let tx = store.begin_read()?;
            let tx = tx.as_ref();

            let Some(root) = load_node(tx, &root_node_id)? else {
                return Ok(None);
            };
            if load_node(tx, &target_node_id)?.is_none() {
                return Ok(None);
            }
            let Some(path_node_ids) = shortest_outward_path(tx, &root_node_id, &target_node_id)?
            else {
                return Ok(None);
            };

            let mut selected = path_node_ids.iter().cloned().collect::<BTreeSet<_>>();
            selected.insert(target_node_id.clone());
            selected.extend(reachable_outward(tx, &target_node_id, subtree_depth)?);

            Ok(Some(ContextPathNeighborhood {
                neighbors: selected_projections(tx, &selected, &root_node_id)?,
                relations: relations_among(tx, &selected)?,
                path_node_ids,
                root,
            }))
        })
        .await
    }
}

impl NodeRelationshipReader for EmbeddedKernelStore {
    async fn load_node_relationships(
        &self,
        node_id: &str,
    ) -> Result<Option<NodeRelationships>, PortError> {
        let node_id = node_id.to_string();
        self.run(move |store| {
            let tx = store.begin_read()?;
            let tx = tx.as_ref();
            if load_node(tx, &node_id)?.is_none() {
                return Ok(None);
            }

            let mut incoming = Vec::new();
            for ((target, source, relation_type), _) in
                tx.scan_str3_by_first(Table::RelationsByTarget, &node_id)?
            {
                let Some(raw) = tx.get(
                    Table::Relations,
                    Key::Str3(&source, &target, &relation_type),
                )?
                else {
                    return Err(PortError::InvalidState(format!(
                        "embedded store adjacency index points at missing relation \
                         `{source}` -> `{target}` ({relation_type})"
                    )));
                };
                incoming.push(NodeRelationProjection {
                    explanation: decode_explanation(&raw)?,
                    source_node_id: source,
                    target_node_id: target,
                    relation_type,
                });
            }

            Ok(Some(NodeRelationships {
                incoming,
                outgoing: outgoing_rows(tx, &node_id)?,
            }))
        })
        .await
    }
}

impl MemoryAboutIndexReader for EmbeddedKernelStore {
    async fn list_memory_abouts(&self) -> Result<Vec<String>, PortError> {
        self.run(|store| {
            let tx = store.begin_read()?;
            Ok(tx
                .scan_str(Table::Anchors)?
                .into_iter()
                .map(|(anchor, _)| anchor)
                .collect())
        })
        .await
    }

    async fn list_memory_abouts_by_dimensions(
        &self,
        dimension_ids: &[String],
    ) -> Result<Vec<String>, PortError> {
        let dimension_ids = dimension_ids.to_vec();
        self.run(move |store| {
            let tx = store.begin_read()?;
            let tx = tx.as_ref();

            let mut abouts = BTreeSet::new();
            for (anchor, _) in tx.scan_str(Table::Anchors)? {
                let is_anchor = load_node(tx, &anchor)?
                    .is_some_and(|node| node.node_kind == MEMORY_ANCHOR_KIND);
                if !is_anchor {
                    continue;
                }
                for relation in outgoing_rows(tx, &anchor)? {
                    if relation.relation_type != "has_dimension" {
                        continue;
                    }
                    let matches =
                        load_node(tx, &relation.target_node_id)?.is_some_and(|dimension| {
                            dimension.node_kind == "memory_dimension"
                                && dimension_ids.iter().any(|dimension_id| {
                                    dimension.node_id == *dimension_id
                                        || dimension
                                            .node_id
                                            .ends_with(&format!(":dimension:{dimension_id}"))
                                })
                        });
                    if matches {
                        abouts.insert(anchor.clone());
                        break;
                    }
                }
            }
            Ok(abouts.into_iter().collect())
        })
        .await
    }
}
