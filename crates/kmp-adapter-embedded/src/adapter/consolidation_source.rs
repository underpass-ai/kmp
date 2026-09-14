use super::{
    engine::{Key, ReadTx, Table},
    serdes::{NodeRecord, decode, decode_explanation, encode},
};
use kmp_domain::{
    PortError,
    consolidation::{ConsolidationSource, MAX_SOURCE_BYTES, MAX_SOURCE_RELATIONS, MAX_SOURCES},
};
use sha2::{Digest, Sha256};

pub(super) fn capture(
    tx: &dyn ReadTx,
    about: &str,
    refs: &[String],
) -> Result<Vec<ConsolidationSource>, PortError> {
    if refs.is_empty()
        || refs.len() > MAX_SOURCES
        || refs.iter().collect::<std::collections::BTreeSet<_>>().len() != refs.len()
    {
        return Err(PortError::InvalidState(
            "capture requires 1..64 distinct source refs".into(),
        ));
    }
    let mut result = Vec::new();
    let mut total = 0u64;
    for reference in refs {
        for table in [Table::Nodes, Table::Details] {
            let size = tx
                .value_len(table, Key::Str(reference))?
                .ok_or_else(|| PortError::InvalidState(format!("missing source: {reference}")))?;
            if size > MAX_SOURCE_BYTES {
                return Err(PortError::InvalidState(format!(
                    "source exceeds capture byte limit: {reference}"
                )));
            }
            total += size;
        }
        if total > 8 * MAX_SOURCE_BYTES {
            return Err(PortError::InvalidState(
                "source capture exceeds 8 MiB".into(),
            ));
        }
        let raw = tx
            .get(Table::Nodes, Key::Str(reference))?
            .ok_or_else(|| PortError::InvalidState("missing node".into()))?;
        let node = decode::<NodeRecord>("consolidation source", &raw)?.into_projection()?;
        if node.properties.get("memory_about").map(String::as_str) != Some(about)
            || !(node.labels.iter().any(|label| label == "entry")
                || matches!(node.node_kind.as_str(), "memory_evidence" | "evidence"))
        {
            return Err(PortError::InvalidState(format!(
                "source does not belong to about: {reference}"
            )));
        }
        let descriptor = super::node_body_descriptor::read_one(tx, reference)?
            .ok_or_else(|| PortError::InvalidState("missing source descriptor".into()))?;
        let body =
            super::node_body_descriptor::read_verified_bodies(tx, std::slice::from_ref(reference))?
                .pop()
                .flatten()
                .ok_or_else(|| PortError::InvalidState("missing body".into()))?;
        let mut rows = tx.scan_str3_page(
            Table::Relations,
            reference,
            None,
            MAX_SOURCE_RELATIONS + 1,
            None,
        )?;
        let incoming = tx.scan_str3_page(
            Table::RelationsByTarget,
            reference,
            None,
            MAX_SOURCE_RELATIONS + 1,
            None,
        )?;
        if rows.len() + incoming.len() > MAX_SOURCE_RELATIONS as usize {
            return Err(PortError::InvalidState(format!(
                "source exceeds relation capture limit: {reference}"
            )));
        }
        for ((target, source, rel), _) in incoming {
            let raw = tx
                .get(Table::Relations, Key::Str3(&source, &target, &rel))?
                .ok_or_else(|| PortError::InvalidState("inconsistent incoming relation".into()))?;
            rows.push(((source, target, rel), raw));
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        rows.dedup_by(|a, b| a.0 == b.0);
        let mut relations = Vec::new();
        for ((source, target, rel), raw) in rows {
            total += raw.len() as u64;
            if total > 8 * MAX_SOURCE_BYTES {
                return Err(PortError::InvalidState(
                    "source capture exceeds 8 MiB".into(),
                ));
            }
            relations.push((
                vec![source, target, rel],
                decode_explanation(&raw)?.to_properties(),
            ));
        }
        let mut provenance = std::collections::BTreeMap::new();
        if let Some(proof) = &node.provenance {
            provenance.insert("source_kind".into(), proof.source_kind().as_str().into());
            if let Some(agent) = proof.source_agent() {
                provenance.insert("source_agent".into(), agent.into());
            }
            if let Some(at) = proof.observed_at() {
                provenance.insert("observed_at".into(), at.into());
            }
        }
        let coordinates = node
            .properties
            .get("payload_coordinates")
            .map(|raw| {
                serde_json::from_str::<Vec<kmp_domain::consolidation::ConsolidationClocks>>(raw)
                    .map_err(|e| {
                        PortError::InvalidState(format!("invalid source coordinates: {e}"))
                    })
            })
            .transpose()?
            .unwrap_or_default();
        let dependency_clocks = relations
            .iter()
            .filter(|(key, fields)| {
                // Relabel memberships live on structural edges without changing
                // payload_coordinates. Their missing clocks must also fail closed.
                key.get(2).is_some_and(|kind| kind == "contains_entry")
                    || [
                        "occurred_at",
                        "observed_at",
                        "ingested_at",
                        "valid_from",
                        "valid_until",
                    ]
                    .iter()
                    .any(|clock| fields.contains_key(*clock))
                    || fields
                        .get("semantic_class")
                        .is_none_or(|kind| kind != "structural")
            })
            .map(
                |(_, fields)| kmp_domain::consolidation::ConsolidationClocks {
                    occurred_at: fields.get("occurred_at").cloned(),
                    observed_at: fields.get("observed_at").cloned(),
                    ingested_at: fields.get("ingested_at").cloned(),
                    valid_from: fields.get("valid_from").cloned(),
                    valid_until: fields.get("valid_until").cloned(),
                },
            )
            .collect();
        let mut source = ConsolidationSource {
            reference: reference.clone(),
            stamp: String::new(),
            body: body.detail,
            status: node.status,
            properties: node.properties,
            provenance,
            coordinates,
            dependency_clocks,
            relations,
        };
        let encoded = encode("source stamp", &(&raw, descriptor.record_digest, &source))?;
        source.stamp = format!("sha256:{:x}", Sha256::digest(encoded));
        result.push(source);
    }
    result.sort_by(|a, b| a.reference.cmp(&b.reference));
    Ok(result)
}
