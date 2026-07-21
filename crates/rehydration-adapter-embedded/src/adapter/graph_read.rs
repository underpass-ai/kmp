use std::collections::{BTreeMap, BTreeSet, VecDeque};

use redb::{ReadOnlyTable, ReadableTable};
use rehydration_domain::{
    ContextPathNeighborhood, GraphNeighborhoodReader, MemoryAboutIndexReader, NodeNeighborhood,
    NodeProjection, NodeRelationProjection, NodeRelationshipReader, NodeRelationships, PortError,
};

use super::projection_write::MEMORY_ANCHOR_KIND;
use super::serdes::{NodeRecord, decode, decode_explanation};
use super::store::{
    ANCHORS, EmbeddedKernelStore, NODES, RELATIONS, RELATIONS_BY_TARGET, range_error,
    storage_error, table_error,
};

type NodeReadTable = ReadOnlyTable<&'static str, &'static [u8]>;
type RelationReadTable = ReadOnlyTable<(&'static str, &'static str, &'static str), &'static [u8]>;

fn load_node(nodes: &NodeReadTable, node_id: &str) -> Result<Option<NodeProjection>, PortError> {
    match nodes.get(node_id).map_err(storage_error)? {
        Some(guard) => Ok(Some(
            decode::<NodeRecord>("graph node", guard.value())?.into_projection()?,
        )),
        None => Ok(None),
    }
}

fn outgoing_rows(
    relations: &RelationReadTable,
    source: &str,
) -> Result<Vec<NodeRelationProjection>, PortError> {
    let upper = format!("{source}\u{0}");
    let mut rows = Vec::new();
    for row in relations
        .range((source, "", "")..(upper.as_str(), "", ""))
        .map_err(range_error)?
    {
        let (key, value) = row.map_err(range_error)?;
        let (source_node_id, target_node_id, relation_type) = key.value();
        rows.push(NodeRelationProjection {
            source_node_id: source_node_id.to_string(),
            target_node_id: target_node_id.to_string(),
            relation_type: relation_type.to_string(),
            explanation: decode_explanation(value.value())?,
        });
    }
    Ok(rows)
}

fn outgoing_targets(relations: &RelationReadTable, source: &str) -> Result<Vec<String>, PortError> {
    let upper = format!("{source}\u{0}");
    let mut targets = Vec::new();
    for row in relations
        .range((source, "", "")..(upper.as_str(), "", ""))
        .map_err(range_error)?
    {
        let (key, _) = row.map_err(range_error)?;
        targets.push(key.value().1.to_string());
    }
    Ok(targets)
}

fn reachable_outward(
    relations: &RelationReadTable,
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
        for target in outgoing_targets(relations, &node_id)? {
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
    relations: &RelationReadTable,
    selected: &BTreeSet<String>,
) -> Result<Vec<NodeRelationProjection>, PortError> {
    let mut rows = Vec::new();
    for source in selected {
        for relation in outgoing_rows(relations, source)? {
            if selected.contains(&relation.target_node_id) {
                rows.push(relation);
            }
        }
    }
    Ok(rows)
}

fn selected_projections(
    nodes: &NodeReadTable,
    selected: &BTreeSet<String>,
    root_node_id: &str,
) -> Result<Vec<NodeProjection>, PortError> {
    let mut projections = Vec::new();
    for node_id in selected {
        if node_id == root_node_id {
            continue;
        }
        if let Some(projection) = load_node(nodes, node_id)? {
            projections.push(projection);
        }
    }
    Ok(projections)
}

fn shortest_outward_path(
    relations: &RelationReadTable,
    root_node_id: &str,
    target_node_id: &str,
) -> Result<Option<Vec<String>>, PortError> {
    let mut predecessors = BTreeMap::<String, String>::new();
    let mut visited = BTreeSet::from([root_node_id.to_string()]);
    let mut frontier = VecDeque::from([root_node_id.to_string()]);

    while let Some(node_id) = frontier.pop_front() {
        for target in outgoing_targets(relations, &node_id)? {
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
            let nodes = tx.open_table(NODES).map_err(table_error)?;
            let relations = tx.open_table(RELATIONS).map_err(table_error)?;

            let Some(root) = load_node(&nodes, &root_node_id)? else {
                return Ok(None);
            };

            let reachable = reachable_outward(&relations, &root_node_id, depth)?;
            // Mirrors the Neo4j neighborhood query: an empty neighborhood
            // reports no relations, even for self-referential root edges.
            let relation_rows = if reachable.is_empty() {
                Vec::new()
            } else {
                let mut selected = reachable.clone();
                selected.insert(root_node_id.clone());
                relations_among(&relations, &selected)?
            };

            Ok(Some(NodeNeighborhood {
                neighbors: selected_projections(&nodes, &reachable, &root_node_id)?,
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
            let nodes = tx.open_table(NODES).map_err(table_error)?;
            let relations = tx.open_table(RELATIONS).map_err(table_error)?;

            let Some(root) = load_node(&nodes, &root_node_id)? else {
                return Ok(None);
            };
            if load_node(&nodes, &target_node_id)?.is_none() {
                return Ok(None);
            }
            let Some(path_node_ids) =
                shortest_outward_path(&relations, &root_node_id, &target_node_id)?
            else {
                return Ok(None);
            };

            let mut selected = path_node_ids.iter().cloned().collect::<BTreeSet<_>>();
            selected.insert(target_node_id.clone());
            selected.extend(reachable_outward(
                &relations,
                &target_node_id,
                subtree_depth,
            )?);

            Ok(Some(ContextPathNeighborhood {
                neighbors: selected_projections(&nodes, &selected, &root_node_id)?,
                relations: relations_among(&relations, &selected)?,
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
            let nodes = tx.open_table(NODES).map_err(table_error)?;
            if load_node(&nodes, &node_id)?.is_none() {
                return Ok(None);
            }
            let relations = tx.open_table(RELATIONS).map_err(table_error)?;
            let by_target = tx.open_table(RELATIONS_BY_TARGET).map_err(table_error)?;

            let mut incoming = Vec::new();
            let upper = format!("{node_id}\u{0}");
            for row in by_target
                .range((node_id.as_str(), "", "")..(upper.as_str(), "", ""))
                .map_err(range_error)?
            {
                let (key, _) = row.map_err(range_error)?;
                let (target, source, relation_type) = key.value();
                let Some(value) = relations
                    .get((source, target, relation_type))
                    .map_err(storage_error)?
                else {
                    return Err(PortError::InvalidState(format!(
                        "embedded store adjacency index points at missing relation \
                         `{source}` -> `{target}` ({relation_type})"
                    )));
                };
                incoming.push(NodeRelationProjection {
                    source_node_id: source.to_string(),
                    target_node_id: target.to_string(),
                    relation_type: relation_type.to_string(),
                    explanation: decode_explanation(value.value())?,
                });
            }

            Ok(Some(NodeRelationships {
                incoming,
                outgoing: outgoing_rows(&relations, &node_id)?,
            }))
        })
        .await
    }
}

impl MemoryAboutIndexReader for EmbeddedKernelStore {
    async fn list_memory_abouts(&self) -> Result<Vec<String>, PortError> {
        self.run(|store| {
            let tx = store.begin_read()?;
            let anchors = tx.open_table(ANCHORS).map_err(table_error)?;
            let mut abouts = Vec::new();
            for row in anchors.iter().map_err(range_error)? {
                let (key, _) = row.map_err(range_error)?;
                abouts.push(key.value().to_string());
            }
            Ok(abouts)
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
            let anchors = tx.open_table(ANCHORS).map_err(table_error)?;
            let nodes = tx.open_table(NODES).map_err(table_error)?;
            let relations = tx.open_table(RELATIONS).map_err(table_error)?;

            let mut abouts = BTreeSet::new();
            for row in anchors.iter().map_err(range_error)? {
                let (key, _) = row.map_err(range_error)?;
                let anchor = key.value().to_string();
                let is_anchor = load_node(&nodes, &anchor)?
                    .is_some_and(|node| node.node_kind == MEMORY_ANCHOR_KIND);
                if !is_anchor {
                    continue;
                }
                for relation in outgoing_rows(&relations, &anchor)? {
                    if relation.relation_type != "has_dimension" {
                        continue;
                    }
                    let matches =
                        load_node(&nodes, &relation.target_node_id)?.is_some_and(|dimension| {
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
