use std::sync::Arc;

use kmp_domain::{
    GraphNeighborhoodReader, NodeDetailProjection, NodeDetailReader, NodeNeighborhood,
};

use crate::ApplicationError;
use crate::queries::{GraphNodeView, QueryApplicationService};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetNodeDetailQuery {
    pub node_id: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct NodeDetailView {
    pub node_id: String,
    pub detail: String,
    pub content_hash: String,
    pub revision: u64,
}

#[derive(Clone, PartialEq, Eq)]
pub struct GetNodeDetailResult {
    pub node: GraphNodeView,
    pub detail: Option<NodeDetailView>,
}

#[derive(Debug)]
pub struct GetNodeDetailUseCase<G, D> {
    graph_reader: G,
    detail_reader: D,
}

impl<G, D> GetNodeDetailUseCase<G, D>
where
    G: GraphNeighborhoodReader + Send + Sync,
    D: NodeDetailReader + Send + Sync,
{
    pub fn new(graph_reader: G, detail_reader: D) -> Self {
        Self {
            graph_reader,
            detail_reader,
        }
    }

    pub async fn execute(&self, node_id: &str) -> Result<GetNodeDetailResult, ApplicationError> {
        let node_id = trim_to_option(node_id)
            .ok_or_else(|| ApplicationError::Validation("node_id cannot be empty".to_string()))?;
        let neighborhood = self.graph_reader.load_neighborhood(&node_id, 1).await?;
        let Some(neighborhood) = neighborhood else {
            return Err(ApplicationError::NotFound(format!(
                "Node not found: {node_id}"
            )));
        };
        #[allow(clippy::manual_map)]
        let node_detail = match self.detail_reader.load_node_detail(&node_id).await? {
            Some(projection) => Some(map_node_detail(projection)),
            None => None,
        };

        Ok(GetNodeDetailResult {
            node: map_root_node(&neighborhood),
            detail: node_detail,
        })
    }
}

impl<G, D, S> QueryApplicationService<G, D, S>
where
    G: GraphNeighborhoodReader + Send + Sync,
    D: NodeDetailReader + Send + Sync,
{
    pub async fn get_node_detail(
        &self,
        query: GetNodeDetailQuery,
    ) -> Result<GetNodeDetailResult, ApplicationError> {
        GetNodeDetailUseCase::new(
            Arc::clone(&self.graph_reader),
            Arc::clone(&self.detail_reader),
        )
        .execute(&query.node_id)
        .await
    }
}

fn map_root_node(neighborhood: &NodeNeighborhood) -> GraphNodeView {
    GraphNodeView {
        node_id: neighborhood.root.node_id.clone(),
        node_kind: neighborhood.root.node_kind.clone(),
        title: neighborhood.root.title.clone(),
        summary: neighborhood.root.summary.clone(),
        status: neighborhood.root.status.clone(),
        labels: neighborhood.root.labels.clone(),
        properties: neighborhood.root.properties.clone(),
    }
}

fn map_node_detail(projection: NodeDetailProjection) -> NodeDetailView {
    NodeDetailView {
        node_id: projection.node_id,
        detail: projection.detail,
        content_hash: projection.content_hash,
        revision: projection.revision,
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use kmp_domain::{
        ContextPathNeighborhood, KmpBundle, NodeDetailProjection, NodeNeighborhood, NodeProjection,
        PortError, SnapshotSaveOptions, SnapshotStore,
    };
    use tokio::sync::Mutex;

    use super::{GetNodeDetailQuery, GetNodeDetailUseCase};
    use crate::ApplicationError;

    struct SeededGraphReader;

    impl kmp_domain::GraphNeighborhoodReader for SeededGraphReader {
        async fn load_neighborhood(
            &self,
            root_node_id: &str,
            _depth: u32,
        ) -> Result<Option<NodeNeighborhood>, PortError> {
            match root_node_id {
                "node-123" => Ok(Some(sample_neighborhood("node-123", "ACTIVE"))),
                "graph-only" => Ok(Some(sample_neighborhood("graph-only", "READY"))),
                _ => Ok(None),
            }
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
            Ok(match node_id {
                "node-123" => Some(NodeDetailProjection {
                    node_id: "node-123".to_string(),
                    detail: "Expanded node detail".to_string(),
                    content_hash: "hash-123".to_string(),
                    revision: 2,
                }),
                "orphan-detail" => Some(NodeDetailProjection {
                    node_id: "orphan-detail".to_string(),
                    detail: "orphaned".to_string(),
                    content_hash: "hash-orphan".to_string(),
                    revision: 1,
                }),
                _ => None,
            })
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

    struct RecordingGraphReader {
        depths: Arc<Mutex<Vec<u32>>>,
    }

    impl kmp_domain::GraphNeighborhoodReader for RecordingGraphReader {
        async fn load_neighborhood(
            &self,
            _root_node_id: &str,
            depth: u32,
        ) -> Result<Option<NodeNeighborhood>, PortError> {
            self.depths.lock().await.push(depth);
            Ok(Some(sample_neighborhood("node-123", "ACTIVE")))
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

    struct EmptyDetailReader;

    impl kmp_domain::NodeDetailReader for EmptyDetailReader {
        async fn load_node_detail(
            &self,
            _node_id: &str,
        ) -> Result<Option<NodeDetailProjection>, PortError> {
            Ok(None)
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
    async fn execute_returns_graph_node_and_detail_when_both_exist() {
        let use_case = GetNodeDetailUseCase::new(SeededGraphReader, SeededDetailReader);

        let result = use_case
            .execute("node-123")
            .await
            .expect("node detail should load");

        assert_eq!(result.node.node_id, "node-123");
        assert_eq!(result.node.node_kind, "task");
        assert_eq!(result.node.title, "Node node-123");
        assert_eq!(
            result
                .detail
                .as_ref()
                .expect("detail should exist")
                .content_hash,
            "hash-123"
        );
    }

    #[tokio::test]
    async fn execute_returns_node_metadata_when_detail_is_missing() {
        let use_case = GetNodeDetailUseCase::new(SeededGraphReader, EmptyDetailReader);

        let result = use_case
            .execute("graph-only")
            .await
            .expect("graph-only node should load");

        assert_eq!(result.node.node_id, "graph-only");
        assert_eq!(result.node.status, "READY");
        assert!(result.detail.is_none());
    }

    #[tokio::test]
    async fn execute_returns_not_found_when_graph_node_is_missing() {
        let use_case = GetNodeDetailUseCase::new(SeededGraphReader, SeededDetailReader);

        let error = match use_case.execute("orphan-detail").await {
            Ok(_) => panic!("orphan detail should not be enough"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            ApplicationError::NotFound(message) if message == "Node not found: orphan-detail"
        ));
    }

    #[tokio::test]
    async fn execute_uses_single_hop_graph_lookup() {
        let depths = Arc::new(Mutex::new(Vec::new()));
        let use_case = GetNodeDetailUseCase::new(
            RecordingGraphReader {
                depths: Arc::clone(&depths),
            },
            EmptyDetailReader,
        );

        let result = use_case
            .execute("node-123")
            .await
            .expect("node detail should load");

        assert_eq!(result.node.node_id, "node-123");
        assert_eq!(&*depths.lock().await, &[1]);
    }

    #[tokio::test]
    async fn execute_rejects_blank_node_id() {
        let use_case = GetNodeDetailUseCase::new(SeededGraphReader, EmptyDetailReader);

        let error = match use_case.execute("   ").await {
            Ok(_) => panic!("blank node id must be rejected"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            ApplicationError::Validation(message) if message == "node_id cannot be empty"
        ));
    }

    fn sample_neighborhood(node_id: &str, status: &str) -> NodeNeighborhood {
        NodeNeighborhood {
            root: NodeProjection {
                node_id: node_id.to_string(),
                node_kind: "task".to_string(),
                title: format!("Node {node_id}"),
                summary: format!("Summary for {node_id}"),
                status: status.to_string(),
                labels: vec!["Task".to_string()],
                properties: BTreeMap::from([("owner".to_string(), "ops".to_string())]),
                provenance: None,
            },
            neighbors: Vec::new(),
            relations: Vec::new(),
        }
    }

    #[tokio::test]
    async fn query_application_service_routes_get_node_detail() {
        let application = crate::queries::QueryApplicationService::new(
            Arc::new(SeededGraphReader),
            Arc::new(SeededDetailReader),
            Arc::new(NoopSnapshotStore),
            "0.1.0",
        );

        let result = application
            .get_node_detail(GetNodeDetailQuery {
                node_id: "node-123".to_string(),
            })
            .await
            .expect("application should route node detail query");

        assert_eq!(result.node.node_id, "node-123");
        assert_eq!(
            result
                .detail
                .as_ref()
                .expect("detail should exist")
                .revision,
            2
        );
    }
}
