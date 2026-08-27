use std::collections::BTreeMap;

use kmp_domain::{
    NodeProjection, NodeRelationProjection, PortError, ProjectionMutation, ProjectionWriter,
};

use super::engine::{Key, Table, WriteTx};
use super::serdes::{DetailRecord, NodeRecord, decode, encode, encode_explanation};
use super::store::EmbeddedKernelStore;

pub(crate) const MEMORY_ANCHOR_KIND: &str = "memory_anchor";

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

fn write_node(tx: &mut dyn WriteTx, node: NodeProjection) -> Result<(), PortError> {
    let node_id = node.node_id.clone();
    let is_anchor = node.node_kind == MEMORY_ANCHOR_KIND;
    let bytes = encode("graph node", &NodeRecord::from(node))?;
    tx.insert(Table::Nodes, Key::Str(&node_id), &bytes)?;
    if is_anchor {
        tx.insert(Table::Anchors, Key::Str(&node_id), &[])
    } else {
        tx.remove(Table::Anchors, Key::Str(&node_id))
    }
}

fn ensure_node(tx: &mut dyn WriteTx, node: NodeProjection) -> Result<(), PortError> {
    if tx.get(Table::Nodes, Key::Str(&node.node_id))?.is_some() {
        return Ok(());
    }
    write_node(tx, node)
}

fn update_node_status(
    tx: &mut dyn WriteTx,
    node_id: &str,
    status: String,
) -> Result<(), PortError> {
    let bytes = tx.get(Table::Nodes, Key::Str(node_id))?.ok_or_else(|| {
        PortError::InvalidState(format!("cannot update missing node `{node_id}`"))
    })?;
    let mut node = decode::<NodeRecord>("graph node", &bytes)?.into_projection()?;
    node.status = status;
    write_node(tx, node)
}

/// Applies a mutation batch inside one transaction. Shared by the live write
/// path and the replay tool so both apply byte-identical projections.
pub(crate) fn apply_mutations_in_transaction(
    tx: &mut dyn WriteTx,
    mutations: Vec<ProjectionMutation>,
) -> Result<u64, PortError> {
    let mut applied = 0u64;
    for mutation in mutations {
        match mutation {
            ProjectionMutation::EnsureNode(node) => {
                ensure_node(tx, node)?;
            }
            ProjectionMutation::UpsertNode(node) => {
                write_node(tx, node)?;
            }
            ProjectionMutation::UpdateNodeStatus { node_id, status } => {
                update_node_status(tx, &node_id, status)?;
            }
            ProjectionMutation::UpsertNodeRelation(relation) => {
                let NodeRelationProjection {
                    source_node_id,
                    target_node_id,
                    relation_type,
                    explanation,
                } = *relation;
                ensure_node(tx, placeholder_node(&source_node_id))?;
                ensure_node(tx, placeholder_node(&target_node_id))?;
                let bytes = encode_explanation(&explanation)?;
                tx.insert(
                    Table::Relations,
                    Key::Str3(&source_node_id, &target_node_id, &relation_type),
                    &bytes,
                )?;
                tx.insert(
                    Table::RelationsByTarget,
                    Key::Str3(&target_node_id, &source_node_id, &relation_type),
                    &[],
                )?;
            }
            ProjectionMutation::UpsertNodeDetail(detail) => {
                let node_id = detail.node_id.clone();
                let bytes = encode("node detail", &DetailRecord::from(detail))?;
                tx.insert(Table::Details, Key::Str(&node_id), &bytes)?;
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
            let mut tx = store.begin_write()?;
            apply_mutations_in_transaction(tx.as_mut(), mutations)?;
            tx.commit()
        })
        .await
    }
}
