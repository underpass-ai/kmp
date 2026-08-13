use kmp_domain::{
    GraphNeighborhoodReader, KmpBundle, NodeDetailReader, SnapshotSaveOptions, SnapshotStore,
};

use crate::ApplicationError;
pub use crate::queries::render_graph_bundle::RenderedContext;
use crate::queries::{
    ContextRenderOptions, QueryApplicationService, QueryTimingBreakdown, RehydrateSessionUseCase,
    clamp_native_graph_traversal_depth, render_graph_bundle_with_options,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetContextQuery {
    pub root_node_id: String,
    pub role: String,
    pub depth: u32,
    pub requested_scopes: Vec<String>,
    pub render_options: ContextRenderOptions,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GetContextResult {
    pub bundle: KmpBundle,
    pub rendered: RenderedContext,
    pub requested_scopes: Vec<String>,
    pub served_at: std::time::SystemTime,
    pub timing: Option<QueryTimingBreakdown>,
}

#[derive(Debug)]
pub struct GetContextUseCase<G, D, S> {
    rehydrate_session: RehydrateSessionUseCase<G, D, S>,
}

impl<G, D, S> GetContextUseCase<G, D, S>
where
    G: GraphNeighborhoodReader + Send + Sync,
    D: NodeDetailReader + Send + Sync,
    S: SnapshotStore + Send + Sync,
{
    pub fn new(rehydrate_session: RehydrateSessionUseCase<G, D, S>) -> Self {
        Self { rehydrate_session }
    }

    pub async fn execute(
        &self,
        root_node_id: &str,
        role: &str,
        depth: u32,
        requested_scopes: &[String],
        render_options: &ContextRenderOptions,
    ) -> Result<GetContextResult, ApplicationError> {
        let (bundle, timing) = self
            .rehydrate_session
            .execute_with_depth(
                root_node_id,
                role,
                clamp_native_graph_traversal_depth(depth),
                false,
                SnapshotSaveOptions::default(),
            )
            .await?;
        let rendered = render_graph_bundle_with_options(&bundle, render_options);

        Ok(GetContextResult {
            bundle,
            rendered,
            requested_scopes: requested_scopes.to_vec(),
            served_at: std::time::SystemTime::now(),
            timing: Some(timing),
        })
    }
}

impl<G, D, S> QueryApplicationService<G, D, S>
where
    G: GraphNeighborhoodReader + Send + Sync,
    D: NodeDetailReader + Send + Sync,
    S: SnapshotStore + Send + Sync,
{
    pub async fn get_context(
        &self,
        query: GetContextQuery,
    ) -> Result<GetContextResult, ApplicationError> {
        let rehydrate = RehydrateSessionUseCase::new(
            std::sync::Arc::clone(&self.graph_reader),
            std::sync::Arc::clone(&self.detail_reader),
            std::sync::Arc::clone(&self.snapshot_store),
            self.generator_version,
        );

        GetContextUseCase::new(rehydrate)
            .execute(
                &query.root_node_id,
                &query.role,
                query.depth,
                &query.requested_scopes,
                &query.render_options,
            )
            .await
    }
}
