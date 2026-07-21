use std::collections::BTreeMap;

use redb::{ReadableTable, WriteTransaction};
use rehydration_domain::{
    NodeProjection, NodeRelationProjection, PortError, ProjectionMutation, ProjectionWriter,
};

use super::serdes::{DetailRecord, NodeRecord, encode, encode_explanation};
use super::store::{
    ANCHORS, DETAILS, EmbeddedKernelStore, NODES, RELATIONS, RELATIONS_BY_TARGET, commit_error,
    storage_error, table_error,
};

pub(crate) const MEMORY_ANCHOR_KIND: &str = "memory_anchor";

type NodeTable<'txn> = redb::Table<'txn, &'static str, &'static [u8]>;
type AnchorTable<'txn> = redb::Table<'txn, &'static str, ()>;

/// Mirrors the Neo4j relation-upsert `MERGE ... ON CREATE` placeholder so the
/// conformance suite observes identical semantics across backends.
fn placeholder_node(node_id: &str) -> NodeProjection {
    NodeProjection {
        node_id: node_id.to_string(),
        node_kind: "placeholder".to_string(),
        title: "[unmaterialized node]".to_string(),
        summary: "Referenced by relation before node materialization".to_string(),
        status: "UNMATERIALIZED".to_string(),
        labels: vec!["placeholder".to_string()],
        properties: BTreeMap::from([
            ("placeholder".to_string(), "true".to_string()),
            (
                "placeholder_reason".to_string(),
                "relation_materialized_before_node".to_string(),
            ),
            (
                "placeholder_created_by_subject".to_string(),
                "graph.relation.materialized".to_string(),
            ),
        ]),
        provenance: None,
    }
}

fn write_node(
    nodes: &mut NodeTable<'_>,
    anchors: &mut AnchorTable<'_>,
    node: NodeProjection,
) -> Result<(), PortError> {
    let node_id = node.node_id.clone();
    let is_anchor = node.node_kind == MEMORY_ANCHOR_KIND;
    let bytes = encode("graph node", &NodeRecord::from(node))?;
    nodes
        .insert(node_id.as_str(), bytes.as_slice())
        .map_err(storage_error)?;
    if is_anchor {
        anchors
            .insert(node_id.as_str(), ())
            .map_err(storage_error)?;
    } else {
        anchors.remove(node_id.as_str()).map_err(storage_error)?;
    }
    Ok(())
}

fn ensure_node(
    nodes: &mut NodeTable<'_>,
    anchors: &mut AnchorTable<'_>,
    node: NodeProjection,
) -> Result<(), PortError> {
    if nodes
        .get(node.node_id.as_str())
        .map_err(storage_error)?
        .is_some()
    {
        return Ok(());
    }
    write_node(nodes, anchors, node)
}

/// Applies a mutation batch inside one transaction. Shared by the live write
/// path and the replay tool so both apply byte-identical projections.
pub(crate) fn apply_mutations_in_transaction(
    tx: &WriteTransaction,
    mutations: Vec<ProjectionMutation>,
) -> Result<u64, PortError> {
    let mut nodes = tx.open_table(NODES).map_err(table_error)?;
    let mut anchors = tx.open_table(ANCHORS).map_err(table_error)?;
    let mut relations = tx.open_table(RELATIONS).map_err(table_error)?;
    let mut relations_by_target = tx.open_table(RELATIONS_BY_TARGET).map_err(table_error)?;
    let mut details = tx.open_table(DETAILS).map_err(table_error)?;

    let mut applied = 0u64;
    for mutation in mutations {
        match mutation {
            ProjectionMutation::EnsureNode(node) => {
                ensure_node(&mut nodes, &mut anchors, node)?;
            }
            ProjectionMutation::UpsertNode(node) => {
                write_node(&mut nodes, &mut anchors, node)?;
            }
            ProjectionMutation::UpsertNodeRelation(relation) => {
                let NodeRelationProjection {
                    source_node_id,
                    target_node_id,
                    relation_type,
                    explanation,
                } = *relation;
                ensure_node(&mut nodes, &mut anchors, placeholder_node(&source_node_id))?;
                ensure_node(&mut nodes, &mut anchors, placeholder_node(&target_node_id))?;
                let bytes = encode_explanation(&explanation)?;
                relations
                    .insert(
                        (
                            source_node_id.as_str(),
                            target_node_id.as_str(),
                            relation_type.as_str(),
                        ),
                        bytes.as_slice(),
                    )
                    .map_err(storage_error)?;
                relations_by_target
                    .insert(
                        (
                            target_node_id.as_str(),
                            source_node_id.as_str(),
                            relation_type.as_str(),
                        ),
                        (),
                    )
                    .map_err(storage_error)?;
            }
            ProjectionMutation::UpsertNodeDetail(detail) => {
                let node_id = detail.node_id.clone();
                let bytes = encode("node detail", &DetailRecord::from(detail))?;
                details
                    .insert(node_id.as_str(), bytes.as_slice())
                    .map_err(storage_error)?;
            }
        }
        applied += 1;
    }

    Ok(applied)
}

impl ProjectionWriter for EmbeddedKernelStore {
    async fn apply_mutations(&self, mutations: Vec<ProjectionMutation>) -> Result<(), PortError> {
        if mutations.is_empty() {
            return Ok(());
        }
        self.run(move |store| {
            let tx = store.begin_write()?;
            apply_mutations_in_transaction(&tx, mutations)?;
            tx.commit().map_err(commit_error)
        })
        .await
    }
}
