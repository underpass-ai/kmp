use std::sync::Arc;

use kmp_domain::{
    BundleRelationship, GraphNeighborhoodReader, KmpBundle, NodeDetailReader, SnapshotSaveOptions,
    SnapshotStore, directed_relationship_path,
};

use crate::ApplicationError;
pub use crate::queries::render_graph_bundle::RenderedContext;
use crate::queries::{
    ContextRenderOptions, MAX_NATIVE_GRAPH_TRAVERSAL_DEPTH, NodeCentricProjectionReader,
    QueryApplicationService, QueryTimingBreakdown, RehydrateSessionUseCase,
    render_graph_bundle_with_options,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetContextPathQuery {
    pub root_node_id: String,
    pub target_node_id: String,
    pub role: String,
    pub subtree_depth: Option<u32>,
    pub render_options: ContextRenderOptions,
    /// Work limits for the bounded bidirectional path search (DESIGN L7).
    /// `None` keeps the unbounded read the kernel query API promises.
    pub limits: Option<kmp_domain::TraceSearchLimits>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GetContextPathResult {
    pub path_bundle: KmpBundle,
    pub rendered: RenderedContext,
    pub served_at: std::time::SystemTime,
    pub timing: Option<QueryTimingBreakdown>,
    root_node_id: String,
    target_node_id: String,
    search: Option<kmp_domain::ContextPathSearch>,
}

impl GetContextPathResult {
    /// The exact directed proof path, in hop order. `None` means the reader
    /// explored the graph but never reached the requested target.
    pub fn path_relationships(&self) -> Option<Vec<&BundleRelationship>> {
        directed_relationship_path(
            self.path_bundle.relationships(),
            &self.root_node_id,
            &self.target_node_id,
        )
    }

    pub fn root_node_id(&self) -> &str {
        &self.root_node_id
    }

    pub fn target_node_id(&self) -> &str {
        &self.target_node_id
    }

    /// The bounded search's report; `None` for the unbounded read.
    pub fn search(&self) -> Option<&kmp_domain::ContextPathSearch> {
        self.search.as_ref()
    }
}

#[derive(Debug)]
pub struct GetContextPathUseCase<G, D, S> {
    graph_reader: G,
    detail_reader: D,
    snapshot_store: S,
    generator_version: &'static str,
    limits: Option<kmp_domain::TraceSearchLimits>,
}

impl<G, D, S> GetContextPathUseCase<G, D, S>
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
            limits: None,
        }
    }

    /// Find the path with the bounded bidirectional search under `limits`.
    pub fn with_limits(mut self, limits: Option<kmp_domain::TraceSearchLimits>) -> Self {
        self.limits = limits;
        self
    }

    pub async fn execute(
        &self,
        root_node_id: &str,
        target_node_id: &str,
        role: &str,
        subtree_depth: Option<u32>,
        render_options: &ContextRenderOptions,
    ) -> Result<GetContextPathResult, ApplicationError> {
        let root_node_id = trim_to_option(root_node_id).ok_or_else(|| {
            ApplicationError::Validation("root_node_id cannot be empty".to_string())
        })?;
        let target_node_id = trim_to_option(target_node_id).ok_or_else(|| {
            ApplicationError::Validation("target_node_id cannot be empty".to_string())
        })?;
        let render_options = focus_target(render_options, &target_node_id);
        let bundle_reader =
            NodeCentricProjectionReader::new(&self.graph_reader, &self.detail_reader);

        let mut search = None;
        let (bundle, timing) = match if root_node_id == target_node_id {
            (None, None)
        } else if let Some(limits) = self.limits {
            let (b, t, found) = bundle_reader
                .load_bounded_context_path_bundle(
                    &root_node_id,
                    &target_node_id,
                    role,
                    self.generator_version,
                    subtree_depth.unwrap_or(MAX_NATIVE_GRAPH_TRAVERSAL_DEPTH),
                    limits,
                )
                .await?;
            search = found;
            (b, Some(t))
        } else {
            let (b, t) = bundle_reader
                .load_context_path_bundle_with_depth(
                    &root_node_id,
                    &target_node_id,
                    role,
                    self.generator_version,
                    subtree_depth.unwrap_or(MAX_NATIVE_GRAPH_TRAVERSAL_DEPTH),
                )
                .await?;
            (b, Some(t))
        } {
            (Some(bundle), timing) => (bundle, timing),
            (None, _) => {
                let (bundle, timing) = RehydrateSessionUseCase::new(
                    &self.graph_reader,
                    &self.detail_reader,
                    &self.snapshot_store,
                    self.generator_version,
                )
                .execute(&target_node_id, role, false, SnapshotSaveOptions::default())
                .await?;
                (bundle, Some(timing))
            }
        };

        let rendered = render_graph_bundle_with_options(&bundle, &render_options);

        Ok(GetContextPathResult {
            path_bundle: bundle,
            rendered,
            served_at: std::time::SystemTime::now(),
            timing,
            root_node_id,
            target_node_id,
            search,
        })
    }
}

impl<G, D, S> QueryApplicationService<G, D, S>
where
    G: GraphNeighborhoodReader + Send + Sync,
    D: NodeDetailReader + Send + Sync,
    S: SnapshotStore + Send + Sync,
{
    pub async fn get_context_path(
        &self,
        query: GetContextPathQuery,
    ) -> Result<GetContextPathResult, ApplicationError> {
        GetContextPathUseCase::new(
            Arc::clone(&self.graph_reader),
            Arc::clone(&self.detail_reader),
            Arc::clone(&self.snapshot_store),
            self.generator_version,
        )
        .with_limits(query.limits)
        .execute(
            &query.root_node_id,
            &query.target_node_id,
            &query.role,
            query.subtree_depth,
            &query.render_options,
        )
        .await
    }
}

fn trim_to_option(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn focus_target(options: &ContextRenderOptions, target_node_id: &str) -> ContextRenderOptions {
    let mut focused = options.clone();
    focused.focus_node_id = Some(target_node_id.to_string());
    focused
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use kmp_domain::{
        ContextPathNeighborhood, KmpBundle, NodeDetailProjection, NodeNeighborhood, NodeProjection,
        NodeRelationProjection, PortError, RelationExplanation, RelationSemanticClass,
        SnapshotSaveOptions, SnapshotStore,
    };
    use tokio::sync::Mutex;

    use super::GetContextPathUseCase;
    use crate::ApplicationError;
    use crate::queries::{ContextRenderOptions, DEFAULT_NATIVE_GRAPH_TRAVERSAL_DEPTH};

    struct SeededGraphReader;

    impl kmp_domain::GraphNeighborhoodReader for SeededGraphReader {
        async fn load_neighborhood(
            &self,
            root_node_id: &str,
            _depth: u32,
        ) -> Result<Option<NodeNeighborhood>, PortError> {
            Ok((root_node_id == "target-node").then_some(NodeNeighborhood {
                root: sample_node("target-node", "task", "Target"),
                neighbors: vec![sample_node("fallback-leaf", "task", "Fallback leaf")],
                relations: vec![NodeRelationProjection {
                    source_node_id: "target-node".to_string(),
                    target_node_id: "fallback-leaf".to_string(),
                    relation_type: "HAS_CHILD".to_string(),
                    explanation: structural_explanation(),
                }],
            }))
        }

        async fn load_context_path(
            &self,
            root_node_id: &str,
            target_node_id: &str,
            _subtree_depth: u32,
        ) -> Result<Option<ContextPathNeighborhood>, PortError> {
            Ok(
                (root_node_id == "root-node" && target_node_id == "target-node").then_some(
                    ContextPathNeighborhood {
                        root: sample_node("root-node", "mission", "Root"),
                        neighbors: vec![
                            sample_node("mid-node", "story", "Middle"),
                            sample_node("target-node", "task", "Target"),
                            sample_node("leaf-node", "artifact", "Leaf"),
                        ],
                        relations: vec![
                            relation("root-node", "mid-node", "HAS_STORY"),
                            relation("mid-node", "target-node", "HAS_TASK"),
                            relation("target-node", "leaf-node", "HAS_ARTIFACT"),
                        ],
                        path_node_ids: vec![
                            "root-node".to_string(),
                            "mid-node".to_string(),
                            "target-node".to_string(),
                        ],
                    },
                ),
            )
        }
    }

    struct RecordingGraphReader {
        neighborhood_calls: Arc<Mutex<Vec<(String, u32)>>>,
    }

    impl kmp_domain::GraphNeighborhoodReader for RecordingGraphReader {
        async fn load_neighborhood(
            &self,
            root_node_id: &str,
            depth: u32,
        ) -> Result<Option<NodeNeighborhood>, PortError> {
            self.neighborhood_calls
                .lock()
                .await
                .push((root_node_id.to_string(), depth));

            Ok((root_node_id == "target-node").then_some(NodeNeighborhood {
                root: sample_node("target-node", "task", "Target"),
                neighbors: vec![sample_node("fallback-leaf", "task", "Fallback leaf")],
                relations: vec![relation("target-node", "fallback-leaf", "HAS_CHILD")],
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
                detail: format!("detail for {node_id}"),
                content_hash: format!("hash-{node_id}"),
                revision: 1,
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

    #[derive(Debug, Default, Clone, Copy)]
    struct NoopSnapshotStore;

    impl SnapshotStore for NoopSnapshotStore {
        async fn save_bundle_with_options(
            &self,
            _bundle: &KmpBundle,
            _options: SnapshotSaveOptions,
        ) -> Result<(), PortError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn execute_builds_a_path_bundle_and_only_loads_path_details() {
        let use_case = GetContextPathUseCase::new(
            SeededGraphReader,
            SeededDetailReader,
            NoopSnapshotStore,
            "0.1.0",
        );

        let result = use_case
            .execute(
                "root-node",
                "target-node",
                "developer",
                None,
                &ContextRenderOptions::default(),
            )
            .await
            .expect("path context should load");

        assert_eq!(result.path_bundle.root_node_id().as_str(), "root-node");
        assert_eq!(result.path_bundle.neighbor_nodes().len(), 3);
        assert_eq!(result.path_bundle.relationships().len(), 3);
        assert_eq!(
            result
                .path_relationships()
                .expect("directed proof path")
                .iter()
                .map(|edge| (edge.source_node_id(), edge.target_node_id()))
                .collect::<Vec<_>>(),
            vec![("root-node", "mid-node"), ("mid-node", "target-node")],
            "the target subtree is context, not an extra proof hop"
        );
        assert_eq!(
            result
                .path_bundle
                .node_details()
                .iter()
                .map(|detail| detail.node_id())
                .collect::<Vec<_>>(),
            vec!["root-node", "mid-node", "target-node"]
        );
        assert!(result.rendered.sections[1].content.contains("Target"));
    }

    #[tokio::test]
    async fn limits_ask_the_reader_for_its_bounded_path_and_keep_the_same_bundle() {
        let use_case = || {
            GetContextPathUseCase::new(
                SeededGraphReader,
                SeededDetailReader,
                NoopSnapshotStore,
                "0.1.0",
            )
        };
        let execute = |limits| async move {
            use_case()
                .with_limits(limits)
                .execute(
                    "root-node",
                    "target-node",
                    "developer",
                    None,
                    &ContextRenderOptions::default(),
                )
                .await
                .expect("path context should load")
        };
        let unbounded = execute(None).await;
        let bounded = execute(Some(kmp_domain::TraceSearchLimits::default())).await;
        assert_eq!(bounded.path_bundle, unbounded.path_bundle);
        assert!(unbounded.search().is_none());
        assert!(
            bounded.search().is_none(),
            "a reader without a bounded read reports no search"
        );
    }

    #[tokio::test]
    async fn execute_falls_back_to_target_context_when_no_path_exists() {
        let neighborhood_calls = Arc::new(Mutex::new(Vec::new()));
        let use_case = GetContextPathUseCase::new(
            RecordingGraphReader {
                neighborhood_calls: Arc::clone(&neighborhood_calls),
            },
            SeededDetailReader,
            NoopSnapshotStore,
            "0.1.0",
        );

        let result = use_case
            .execute(
                "root-node",
                "target-node",
                "developer",
                None,
                &ContextRenderOptions::default(),
            )
            .await
            .expect("fallback context should load");

        assert_eq!(result.path_bundle.root_node_id().as_str(), "target-node");
        assert!(
            result.path_relationships().is_none(),
            "target fallback context must not masquerade as a path from the root"
        );
        assert_eq!(
            &*neighborhood_calls.lock().await,
            &[(
                "target-node".to_string(),
                DEFAULT_NATIVE_GRAPH_TRAVERSAL_DEPTH
            )]
        );
    }

    #[tokio::test]
    async fn execute_rejects_blank_target_node_ids() {
        let use_case = GetContextPathUseCase::new(
            SeededGraphReader,
            SeededDetailReader,
            NoopSnapshotStore,
            "0.1.0",
        );

        let error = use_case
            .execute(
                "root-node",
                "   ",
                "developer",
                None,
                &ContextRenderOptions::default(),
            )
            .await
            .expect_err("blank target ids must fail");

        assert!(matches!(
            error,
            ApplicationError::Validation(message) if message == "target_node_id cannot be empty"
        ));
    }

    fn sample_node(node_id: &str, node_kind: &str, title: &str) -> NodeProjection {
        NodeProjection {
            node_id: node_id.to_string(),
            node_kind: node_kind.to_string(),
            title: title.to_string(),
            summary: format!("{title} summary"),
            status: "ACTIVE".to_string(),
            labels: vec![node_kind.to_string()],
            properties: BTreeMap::new(),
            provenance: None,
        }
    }

    fn relation(
        source_node_id: &str,
        target_node_id: &str,
        relation_type: &str,
    ) -> NodeRelationProjection {
        NodeRelationProjection {
            source_node_id: source_node_id.to_string(),
            target_node_id: target_node_id.to_string(),
            relation_type: relation_type.to_string(),
            explanation: structural_explanation(),
        }
    }

    fn structural_explanation() -> RelationExplanation {
        RelationExplanation::new(RelationSemanticClass::Structural)
    }
}
