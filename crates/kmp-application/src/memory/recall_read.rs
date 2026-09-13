//! Admit the recall catalogue before materializing canonical candidate text.
use super::*;
use crate::queries::QueryTimingBreakdown;
use std::time::{Duration, Instant};

impl<G, D, S, E, W> KernelMemoryApplicationService<G, D, S, E, W>
where
    G: GraphNeighborhoodReader + MemoryAboutIndexReader + NodeRelationshipReader + Send + Sync,
    D: NodeDetailReader + Send + Sync,
    S: SnapshotStore + Send + Sync,
    E: ContextEventStore + Send + Sync,
    W: ProjectionWriter + Send + Sync,
{
    pub(super) async fn selected_recall_context(
        &self,
        about: &str,
        role: &str,
        depth: u32,
        dimensions: &DimensionSelection,
        options: &ContextRenderOptions,
    ) -> Result<GetContextResult, ApplicationError> {
        let roots = self.memory_context_roots(about, dimensions).await?;
        let requested_scopes = requested_dimension_scopes(about, dimensions, &roots);
        let mut catalogues = Vec::with_capacity(roots.len());
        let mut timing = QueryTimingBreakdown::not_found(Duration::ZERO);
        timing.role_count = roots.len();
        for root in roots {
            let (catalogue, read) = self
                .query_application
                .read_context_catalogue_with_timing(
                    &kmp_domain::NeighborhoodRequest::new(
                        root,
                        crate::queries::clamp_native_graph_traversal_depth(depth),
                    ),
                    role,
                )
                .await?;
            timing.graph_load += read.graph_load;
            timing.bundle_assembly += read.bundle_assembly;
            catalogues.push(catalogue);
        }
        let selection_start = Instant::now();
        let catalogue = crate::memory::merge_memory_bundles::merge(catalogues)?;
        // Membership needs the whole entry's labels. Body length or a compact
        // card cannot decide lexical eligibility: every admitted candidate
        // keeps its complete canonical text before ranking.
        let selected = filter_bundle_by_memory_dimensions(&catalogue, dimensions)?;
        let ids = bundle_node_ids(&selected);
        timing.bundle_assembly += selection_start.elapsed();
        timing.batch_size = ids.len();
        let nodes_start = Instant::now();
        let selected = self
            .query_application
            .materialize_selected_nodes(selected, &ids)
            .await?;
        timing.graph_load += nodes_start.elapsed();
        let details_start = Instant::now();
        let bundle = self
            .query_application
            .materialize_selected_details(selected, &ids)
            .await?;
        timing.detail_load = details_start.elapsed();
        let rendered = render_graph_bundle_with_options(&bundle, options);
        Ok(GetContextResult {
            bundle,
            read_revision: self.query_application.graph_read_revision().await?,
            rendered,
            requested_scopes,
            served_at: std::time::SystemTime::now(),
            timing: Some(timing),
        })
    }
}
