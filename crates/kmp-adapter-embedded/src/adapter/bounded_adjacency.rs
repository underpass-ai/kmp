use kmp_domain::{
    AdjacencyPage, AdjacencyRequest, BoundedRelationReader, NodeRelationProjection, PortError,
    RelationDirection, RelationPosition,
};

use super::{
    engine::{Key, ReadTx, Table},
    serdes::decode_explanation,
    store::EmbeddedKernelStore,
};

/// Reusable inside one traversal's transaction; no second snapshot is opened.
pub(super) fn read_page(
    tx: &dyn ReadTx,
    request: &AdjacencyRequest,
) -> Result<AdjacencyPage, PortError> {
    let table = match request.direction() {
        RelationDirection::Outgoing => Table::Relations,
        RelationDirection::Incoming => Table::RelationsByTarget,
    };
    let after = request
        .after()
        .map(|p| (p.neighbor.as_str(), p.relation.as_str()));
    if request
        .relation_type()
        .is_some_and(|kind| request.after().is_some_and(|p| p.relation != kind))
    {
        return Err(PortError::InvalidState(
            "adjacency position belongs to another relation type".into(),
        ));
    }
    let rows = tx.scan_str3_page(
        table,
        request.node_id(),
        after,
        request.limit(),
        request.relation_type(),
    )?;
    let exhausted = rows.len() < request.limit() as usize;
    let next = if exhausted {
        None
    } else {
        rows.last()
            .map(|((_, neighbor, relation), _)| RelationPosition {
                neighbor: neighbor.clone(),
                relation: relation.clone(),
            })
    };
    let mut edges = Vec::with_capacity(rows.len());
    for ((first, second, relation_type), value) in rows {
        let (source_node_id, target_node_id, raw) = match request.direction() {
            RelationDirection::Outgoing => (first, second, value),
            RelationDirection::Incoming => {
                let raw = tx
                    .get(Table::Relations, Key::Str3(&second, &first, &relation_type))?
                    .ok_or_else(|| {
                        PortError::InvalidState(
                            "adjacency index points at a missing relation".into(),
                        )
                    })?;
                (second, first, raw)
            }
        };
        edges.push(NodeRelationProjection {
            source_node_id,
            target_node_id,
            relation_type,
            explanation: decode_explanation(&raw)?,
        });
    }
    Ok(AdjacencyPage {
        edges,
        next,
        exhausted,
    })
}

impl BoundedRelationReader for EmbeddedKernelStore {
    async fn read_adjacency(&self, request: &AdjacencyRequest) -> Result<AdjacencyPage, PortError> {
        let request = request.clone();
        self.run(move |store| {
            let tx = store.begin_read()?;
            read_page(tx.as_ref(), &request)
        })
        .await
    }
}

#[cfg(test)]
#[path = "bounded_adjacency_tests.rs"]
mod tests;
