use std::collections::BTreeSet;

use kmp_domain::{MemoryAboutIndexReader, PortError};

use super::dimension_lookup_header::DimensionLookupHeader;
use super::engine::{LinkedJsonScan, Table};
use super::projection_write::MEMORY_ANCHOR_KIND;
use super::store::EmbeddedKernelStore;

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
        let terms: BTreeSet<_> = dimension_ids.iter().cloned().collect();
        self.run(move |store| {
            let tx = store.begin_read()?;
            let mut abouts = BTreeSet::new();
            let mut after: Option<(String, String)> = None;
            loop {
                let rows = tx.scan_linked_json(&LinkedJsonScan {
                    roots: Table::Anchors,
                    links: Table::Relations,
                    records: Table::Nodes,
                    relation: "has_dimension",
                    fields: &["node_id", "node_kind", "properties.dimension_kind"],
                    after: after
                        .as_ref()
                        .map(|(source, target)| (source.as_str(), target.as_str())),
                    limit: 256,
                })?;
                let count = rows.len();
                for row in rows {
                    after = Some((row.source.clone(), row.target));
                    if abouts.contains(&row.source) {
                        continue;
                    }
                    let Some(source) = DimensionLookupHeader::read(row.source_json.as_deref())?
                    else {
                        continue;
                    };
                    if source.node_kind != MEMORY_ANCHOR_KIND {
                        continue;
                    }
                    if DimensionLookupHeader::read(row.target_json.as_deref())?
                        .is_some_and(|target| target.matches(&terms))
                    {
                        abouts.insert(row.source);
                    }
                }
                if count < 256 {
                    break;
                }
            }
            Ok(abouts.into_iter().collect())
        })
        .await
    }
}
