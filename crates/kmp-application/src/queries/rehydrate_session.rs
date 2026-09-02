use std::sync::Arc;
use std::time::SystemTime;

use kmp_domain::{
    BundleMetadata, GraphNeighborhoodReader, KmpBundle, NeighborhoodRequest, NodeDetailReader,
    SnapshotSaveOptions, SnapshotStore,
};

use crate::ApplicationError;
use crate::queries::{
    ContextRenderOptions, NodeCentricProjectionReader, QueryApplicationService,
    QueryTimingBreakdown,
    context_render_options::EndpointHint,
    render_graph_bundle::{RenderedContext, render_graph_bundle_with_options},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RehydrateSessionQuery {
    pub root_node_id: String,
    pub roles: Vec<String>,
    pub persist_snapshot: bool,
    pub timeline_window: u32,
    pub snapshot_ttl_seconds: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RehydrateSessionResult {
    pub root_node_id: String,
    pub bundles: Vec<KmpBundle>,
    /// Per-role rendered context with quality metrics, tiers, and truncation.
    pub rendered_contexts: Vec<RenderedContext>,
    pub timeline_events: u32,
    pub version: BundleMetadata,
    pub snapshot_persisted: bool,
    pub snapshot_id: Option<String>,
    pub generated_at: SystemTime,
    pub timing: Option<QueryTimingBreakdown>,
}

#[derive(Debug)]
pub struct RehydrateSessionUseCase<G, D, S> {
    graph_reader: G,
    detail_reader: D,
    snapshot_store: S,
    generator_version: &'static str,
}

impl<G, D, S> RehydrateSessionUseCase<G, D, S>
where
    G: GraphNeighborhoodReader + Send + Sync,
    D: NodeDetailReader + Send + Sync,
    S: SnapshotStore + Send + Sync,
{
    pub fn new(
        graph_reader: G,
        detail_reader: D,
        snapshot_store: S,
        generator_version: &'static str,
    ) -> Self {
        Self {
            graph_reader,
            detail_reader,
            snapshot_store,
            generator_version,
        }
    }

    pub async fn execute(
        &self,
        root_node_id: &str,
        role: &str,
        persist_snapshot: bool,
        snapshot_options: SnapshotSaveOptions,
    ) -> Result<(KmpBundle, QueryTimingBreakdown), ApplicationError> {
        self.execute_with_depth(
            root_node_id,
            role,
            crate::queries::DEFAULT_NATIVE_GRAPH_TRAVERSAL_DEPTH,
            persist_snapshot,
            snapshot_options,
        )
        .await
    }

    pub async fn execute_with_depth(
        &self,
        root_node_id: &str,
        role: &str,
        depth: u32,
        persist_snapshot: bool,
        snapshot_options: SnapshotSaveOptions,
    ) -> Result<(KmpBundle, QueryTimingBreakdown), ApplicationError> {
        self.execute_for(
            &NeighborhoodRequest::new(root_node_id, depth),
            role,
            persist_snapshot,
            snapshot_options,
        )
        .await
    }

    /// Rehydrates for a request that already names the axes the caller
    /// resolved, so the store can narrow instead of the caller discarding.
    pub async fn execute_for(
        &self,
        request: &NeighborhoodRequest,
        role: &str,
        persist_snapshot: bool,
        snapshot_options: SnapshotSaveOptions,
    ) -> Result<(KmpBundle, QueryTimingBreakdown), ApplicationError> {
        let root_node_id = request.root_node_id();
        let bundle_reader =
            NodeCentricProjectionReader::new(&self.graph_reader, &self.detail_reader);
        let (bundle, timing) = match bundle_reader
            .load_bundle_for(request, role, self.generator_version)
            .await?
        {
            (Some(bundle), timing) => (bundle, timing),
            (None, _) => {
                return Err(ApplicationError::NotFound(format!(
                    "node '{}' not found",
                    root_node_id
                )));
            }
        };

        if persist_snapshot {
            self.snapshot_store
                .save_bundle_with_options(&bundle, snapshot_options)
                .await?;
        }
        Ok((bundle, timing))
    }
}

impl<G, D, S> QueryApplicationService<G, D, S>
where
    G: GraphNeighborhoodReader + Send + Sync,
    D: NodeDetailReader + Send + Sync,
    S: SnapshotStore + Send + Sync,
{
    pub async fn rehydrate_session(
        &self,
        query: RehydrateSessionQuery,
    ) -> Result<RehydrateSessionResult, ApplicationError> {
        if query.roles.is_empty() {
            return Err(ApplicationError::Validation(
                "roles cannot be empty".to_string(),
            ));
        }

        let bundle_reader = NodeCentricProjectionReader::new(
            self.graph_reader.as_ref(),
            self.detail_reader.as_ref(),
        );
        let snapshot_options = SnapshotSaveOptions::new(Some(query.snapshot_ttl_seconds));

        let (bundles, timing) = bundle_reader
            .load_bundles_for_roles(
                &query.root_node_id,
                &query.roles,
                self.generator_version,
                crate::queries::DEFAULT_NATIVE_GRAPH_TRAVERSAL_DEPTH,
            )
            .await?;

        let bundles = match bundles {
            Some(bundles) => bundles,
            None => {
                return Err(ApplicationError::NotFound(format!(
                    "node '{}' not found",
                    query.root_node_id
                )));
            }
        };

        let session_options = ContextRenderOptions {
            endpoint_hint: EndpointHint::SessionSnapshot,
            ..Default::default()
        };
        let rendered_contexts: Vec<RenderedContext> = bundles
            .iter()
            .map(|b| render_graph_bundle_with_options(b, &session_options))
            .collect();

        if query.persist_snapshot {
            for bundle in &bundles {
                self.snapshot_store
                    .save_bundle_with_options(bundle, snapshot_options)
                    .await?;
            }
        }

        let snapshot_id = if query.persist_snapshot {
            Some(format!(
                "snapshot:{}:{}",
                query.root_node_id,
                query.roles.join(",")
            ))
        } else {
            None
        };

        Ok(RehydrateSessionResult {
            root_node_id: query.root_node_id,
            bundles,
            rendered_contexts,
            timeline_events: query.timeline_window,
            version: BundleMetadata::initial(self.generator_version),
            snapshot_persisted: query.persist_snapshot,
            snapshot_id,
            generated_at: SystemTime::now(),
            timing: Some(timing),
        })
    }

    pub async fn warmup_bundle(&self) -> Result<KmpBundle, ApplicationError> {
        let (bundle, _timing) = RehydrateSessionUseCase::new(
            Arc::clone(&self.graph_reader),
            Arc::clone(&self.detail_reader),
            Arc::clone(&self.snapshot_store),
            self.generator_version,
        )
        .execute(
            "bootstrap-node",
            "system",
            false,
            SnapshotSaveOptions::default(),
        )
        .await?;
        Ok(bundle)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use tokio::sync::Mutex;

    use kmp_domain::{
        ContextPathNeighborhood, NodeDetailProjection, NodeNeighborhood, NodeProjection, PortError,
        SnapshotSaveOptions,
    };

    use super::{QueryApplicationService, RehydrateSessionQuery};

    struct SeededGraphReader;

    impl kmp_domain::GraphNeighborhoodReader for SeededGraphReader {
        async fn load_neighborhood(
            &self,
            root_node_id: &str,
            _depth: u32,
        ) -> Result<Option<NodeNeighborhood>, PortError> {
            Ok(Some(NodeNeighborhood {
                root: NodeProjection {
                    node_id: root_node_id.to_string(),
                    node_kind: "story".to_string(),
                    title: "Root".to_string(),
                    summary: "Root summary".to_string(),
                    status: "ACTIVE".to_string(),
                    labels: vec!["Story".to_string()],
                    properties: BTreeMap::new(),
                    provenance: None,
                },
                neighbors: Vec::new(),
                relations: Vec::new(),
            }))
        }

        async fn load_context_path(
            &self,
            _root_node_id: &str,
            _target_node_id: &str,
            _subtree_depth: u32,
        ) -> Result<Option<ContextPathNeighborhood>, PortError> {
            Ok(None)
        }
    }

    struct SeededDetailReader;

    impl kmp_domain::NodeDetailReader for SeededDetailReader {
        async fn load_node_detail(
            &self,
            node_id: &str,
        ) -> Result<Option<NodeDetailProjection>, PortError> {
            Ok(Some(NodeDetailProjection {
                node_id: node_id.to_string(),
                detail: "Expanded detail".to_string(),
                content_hash: "hash-1".to_string(),
                revision: 2,
            }))
        }

        async fn load_node_details_batch(
            &self,
            node_ids: Vec<String>,
        ) -> Result<Vec<Option<NodeDetailProjection>>, PortError> {
            let mut results = Vec::with_capacity(node_ids.len());
            for node_id in &node_ids {
                results.push(self.load_node_detail(node_id).await?);
            }
            Ok(results)
        }
    }

    #[derive(Debug, Default)]
    struct RecordingSnapshotStore {
        options: Mutex<Vec<SnapshotSaveOptions>>,
    }

    impl kmp_domain::SnapshotStore for RecordingSnapshotStore {
        async fn save_bundle_with_options(
            &self,
            _bundle: &kmp_domain::KmpBundle,
            options: SnapshotSaveOptions,
        ) -> Result<(), PortError> {
            self.options.lock().await.push(options);
            Ok(())
        }
    }

    #[tokio::test]
    async fn rehydrate_session_propagates_snapshot_ttl_to_store() {
        let snapshot_store = Arc::new(RecordingSnapshotStore::default());
        let service = QueryApplicationService::new(
            Arc::new(SeededGraphReader),
            Arc::new(SeededDetailReader),
            Arc::clone(&snapshot_store),
            "0.1.0",
        );

        let result = service
            .rehydrate_session(RehydrateSessionQuery {
                root_node_id: "story-123".to_string(),
                roles: vec!["developer".to_string()],
                persist_snapshot: true,
                timeline_window: 50,
                snapshot_ttl_seconds: 1800,
            })
            .await
            .expect("rehydration should succeed");

        assert!(result.snapshot_persisted);
        assert_eq!(
            snapshot_store.options.lock().await.as_slice(),
            &[SnapshotSaveOptions::new(Some(1800))]
        );
    }

    struct EmptyGraphReader;

    impl kmp_domain::GraphNeighborhoodReader for EmptyGraphReader {
        async fn load_neighborhood(
            &self,
            _root_node_id: &str,
            _depth: u32,
        ) -> Result<Option<NodeNeighborhood>, PortError> {
            Ok(None)
        }

        async fn load_context_path(
            &self,
            _root_node_id: &str,
            _target_node_id: &str,
            _subtree_depth: u32,
        ) -> Result<Option<ContextPathNeighborhood>, PortError> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn rehydrate_session_returns_not_found_when_node_does_not_exist() {
        let service = QueryApplicationService::new(
            Arc::new(EmptyGraphReader),
            Arc::new(SeededDetailReader),
            Arc::new(RecordingSnapshotStore::default()),
            "0.1.0",
        );

        let result = service
            .rehydrate_session(RehydrateSessionQuery {
                root_node_id: "nonexistent-node".to_string(),
                roles: vec!["developer".to_string()],
                persist_snapshot: false,
                timeline_window: 0,
                snapshot_ttl_seconds: 0,
            })
            .await;

        match result {
            Err(crate::ApplicationError::NotFound(_)) => {}
            other => panic!("expected NotFound, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn rehydrate_session_multi_role_returns_one_bundle_per_role() {
        let service = QueryApplicationService::new(
            Arc::new(SeededGraphReader),
            Arc::new(SeededDetailReader),
            Arc::new(RecordingSnapshotStore::default()),
            "0.1.0",
        );

        let result = service
            .rehydrate_session(RehydrateSessionQuery {
                root_node_id: "story-123".to_string(),
                roles: vec![
                    "developer".to_string(),
                    "reviewer".to_string(),
                    "ops".to_string(),
                ],
                persist_snapshot: false,
                timeline_window: 0,
                snapshot_ttl_seconds: 0,
            })
            .await
            .expect("multi-role rehydration should succeed");

        assert_eq!(result.bundles.len(), 3);
        assert_eq!(result.bundles[0].role().as_str(), "developer");
        assert_eq!(result.bundles[1].role().as_str(), "reviewer");
        assert_eq!(result.bundles[2].role().as_str(), "ops");
        // All bundles share the same graph — root and details are identical.
        for bundle in &result.bundles {
            assert_eq!(bundle.root_node().node_id(), "story-123");
            assert_eq!(bundle.node_details().len(), 1);
        }
    }

    #[tokio::test]
    async fn rehydrate_session_multi_role_not_found_returns_error_not_partial() {
        let service = QueryApplicationService::new(
            Arc::new(EmptyGraphReader),
            Arc::new(SeededDetailReader),
            Arc::new(RecordingSnapshotStore::default()),
            "0.1.0",
        );

        let result = service
            .rehydrate_session(RehydrateSessionQuery {
                root_node_id: "nonexistent".to_string(),
                roles: vec!["developer".to_string(), "reviewer".to_string()],
                persist_snapshot: false,
                timeline_window: 0,
                snapshot_ttl_seconds: 0,
            })
            .await;

        match result {
            Err(crate::ApplicationError::NotFound(_)) => {}
            other => panic!("expected NotFound for all roles, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn rehydrate_session_multi_role_persists_snapshot_per_bundle() {
        let snapshot_store = Arc::new(RecordingSnapshotStore::default());
        let service = QueryApplicationService::new(
            Arc::new(SeededGraphReader),
            Arc::new(SeededDetailReader),
            Arc::clone(&snapshot_store),
            "0.1.0",
        );

        let result = service
            .rehydrate_session(RehydrateSessionQuery {
                root_node_id: "story-123".to_string(),
                roles: vec!["developer".to_string(), "reviewer".to_string()],
                persist_snapshot: true,
                timeline_window: 0,
                snapshot_ttl_seconds: 900,
            })
            .await
            .expect("multi-role rehydration should succeed");

        assert!(result.snapshot_persisted);
        assert_eq!(
            result.snapshot_id,
            Some("snapshot:story-123:developer,reviewer".to_string())
        );
        // One save per role bundle.
        assert_eq!(snapshot_store.options.lock().await.len(), 2);
    }

    #[tokio::test]
    async fn rehydrate_session_empty_roles_returns_validation_error() {
        let service = QueryApplicationService::new(
            Arc::new(SeededGraphReader),
            Arc::new(SeededDetailReader),
            Arc::new(RecordingSnapshotStore::default()),
            "0.1.0",
        );

        let result = service
            .rehydrate_session(RehydrateSessionQuery {
                root_node_id: "story-123".to_string(),
                roles: vec![],
                persist_snapshot: false,
                timeline_window: 0,
                snapshot_ttl_seconds: 0,
            })
            .await;

        match result {
            Err(crate::ApplicationError::Validation(msg)) => {
                assert!(msg.contains("roles"), "error should mention roles: {msg}");
            }
            other => panic!("expected Validation error, got: {other:?}"),
        }
    }
}
