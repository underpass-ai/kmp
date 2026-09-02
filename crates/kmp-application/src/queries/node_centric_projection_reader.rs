use std::collections::BTreeSet;
use std::time::Instant;

use kmp_domain::{
    BundleMetadata, BundleNode, BundleNodeDetail, BundleRelationship, CaseId,
    GraphNeighborhoodReader, KmpBundle, NeighborhoodRequest, NodeDetailReader, NodeNeighborhood,
    NodeProjection, Role,
};

use crate::ApplicationError;
use crate::queries::QueryTimingBreakdown;
use crate::queries::ordered_neighborhood::ordered_neighborhood;
use crate::queries::{
    DEFAULT_NATIVE_GRAPH_TRAVERSAL_DEPTH, clamp_native_graph_subtree_depth,
    clamp_native_graph_traversal_depth,
};

#[derive(Debug, Clone)]
pub struct NodeCentricProjectionReader<G, D> {
    graph_reader: G,
    detail_reader: D,
}

impl<G, D> NodeCentricProjectionReader<G, D> {
    pub fn new(graph_reader: G, detail_reader: D) -> Self {
        Self {
            graph_reader,
            detail_reader,
        }
    }
}

impl<G, D> NodeCentricProjectionReader<G, D>
where
    G: GraphNeighborhoodReader + Send + Sync,
    D: NodeDetailReader + Send + Sync,
{
    pub async fn load_bundle(
        &self,
        root_node_id: &str,
        role: &str,
        generator_version: &str,
    ) -> Result<(Option<KmpBundle>, QueryTimingBreakdown), ApplicationError> {
        self.load_bundle_with_depth(
            root_node_id,
            role,
            generator_version,
            DEFAULT_NATIVE_GRAPH_TRAVERSAL_DEPTH,
        )
        .await
    }

    pub async fn load_bundle_with_depth(
        &self,
        root_node_id: &str,
        role: &str,
        generator_version: &str,
        depth: u32,
    ) -> Result<(Option<KmpBundle>, QueryTimingBreakdown), ApplicationError> {
        self.load_bundle_for(
            &NeighborhoodRequest::new(root_node_id, clamp_native_graph_traversal_depth(depth)),
            role,
            generator_version,
        )
        .await
    }

    /// Loads the bundle for a request that already carries the axes the caller
    /// resolved, so a store able to narrow on them does not have to be asked
    /// for what will be thrown away.
    pub async fn load_bundle_for(
        &self,
        request: &NeighborhoodRequest,
        role: &str,
        generator_version: &str,
    ) -> Result<(Option<KmpBundle>, QueryTimingBreakdown), ApplicationError> {
        let root_node_id = request.root_node_id();
        let graph_start = Instant::now();
        let Some(neighborhood) = self.graph_reader.load_scoped_neighborhood(request).await? else {
            return Ok((None, QueryTimingBreakdown::not_found(graph_start.elapsed())));
        };
        if is_placeholder_projection_node(&neighborhood.root) {
            return Ok((None, QueryTimingBreakdown::not_found(graph_start.elapsed())));
        }
        let neighborhood = ordered_neighborhood(filter_placeholder_nodes(neighborhood));
        let graph_load = graph_start.elapsed();

        let batch_size = 1 + neighborhood.neighbors.len();

        let detail_start = Instant::now();
        let node_details = load_node_details(&self.detail_reader, &neighborhood).await?;
        let detail_load = detail_start.elapsed();

        let assembly_start = Instant::now();
        let bundle = build_bundle(
            root_node_id,
            role,
            generator_version,
            neighborhood,
            node_details,
        )?;
        let bundle_assembly = assembly_start.elapsed();

        let timing = QueryTimingBreakdown {
            graph_load,
            detail_load,
            bundle_assembly,
            role_count: 1,
            batch_size,
        };

        Ok((Some(bundle), timing))
    }

    pub async fn load_bundles_for_roles(
        &self,
        root_node_id: &str,
        roles: &[String],
        generator_version: &str,
        depth: u32,
    ) -> Result<(Option<Vec<KmpBundle>>, QueryTimingBreakdown), ApplicationError> {
        let graph_start = Instant::now();
        let Some(neighborhood) = self
            .graph_reader
            .load_neighborhood(root_node_id, clamp_native_graph_traversal_depth(depth))
            .await?
        else {
            return Ok((None, QueryTimingBreakdown::not_found(graph_start.elapsed())));
        };
        if is_placeholder_projection_node(&neighborhood.root) {
            return Ok((None, QueryTimingBreakdown::not_found(graph_start.elapsed())));
        }
        let neighborhood = ordered_neighborhood(filter_placeholder_nodes(neighborhood));
        let graph_load = graph_start.elapsed();

        let batch_size = 1 + neighborhood.neighbors.len();

        let detail_start = Instant::now();
        let node_details = load_node_details(&self.detail_reader, &neighborhood).await?;
        let detail_load = detail_start.elapsed();

        let assembly_start = Instant::now();
        let mut bundles = Vec::with_capacity(roles.len());
        for role in roles {
            bundles.push(build_bundle(
                root_node_id,
                role,
                generator_version,
                neighborhood.clone(),
                node_details.clone(),
            )?);
        }
        let bundle_assembly = assembly_start.elapsed();

        let timing = QueryTimingBreakdown {
            graph_load,
            detail_load,
            bundle_assembly,
            role_count: roles.len(),
            batch_size,
        };

        Ok((Some(bundles), timing))
    }

    pub async fn load_context_path_bundle_with_depth(
        &self,
        root_node_id: &str,
        target_node_id: &str,
        role: &str,
        generator_version: &str,
        subtree_depth: u32,
    ) -> Result<(Option<KmpBundle>, QueryTimingBreakdown), ApplicationError> {
        let graph_start = Instant::now();
        let Some(path_neighborhood) = self
            .graph_reader
            .load_context_path(
                root_node_id,
                target_node_id,
                clamp_native_graph_subtree_depth(subtree_depth),
            )
            .await?
        else {
            return Ok((None, QueryTimingBreakdown::not_found(graph_start.elapsed())));
        };

        if is_placeholder_projection_node(&path_neighborhood.root) {
            return Ok((None, QueryTimingBreakdown::not_found(graph_start.elapsed())));
        }

        let neighborhood = filter_placeholder_nodes(NodeNeighborhood {
            root: path_neighborhood.root,
            neighbors: path_neighborhood.neighbors,
            relations: path_neighborhood.relations,
        });
        let allowed_node_ids: BTreeSet<String> = std::iter::once(neighborhood.root.node_id.clone())
            .chain(
                neighborhood
                    .neighbors
                    .iter()
                    .map(|node| node.node_id.clone()),
            )
            .collect();
        let path_node_ids = path_neighborhood
            .path_node_ids
            .into_iter()
            .filter(|node_id| allowed_node_ids.contains(node_id))
            .collect::<Vec<_>>();
        let neighborhood = ordered_neighborhood(neighborhood);
        let graph_load = graph_start.elapsed();

        let batch_size = path_node_ids.len();

        let detail_start = Instant::now();
        let node_details = load_node_details_for_ids(&self.detail_reader, path_node_ids).await?;
        let detail_load = detail_start.elapsed();

        let assembly_start = Instant::now();
        let bundle = build_bundle(
            root_node_id,
            role,
            generator_version,
            neighborhood,
            node_details,
        )?;
        let bundle_assembly = assembly_start.elapsed();

        let timing = QueryTimingBreakdown {
            graph_load,
            detail_load,
            bundle_assembly,
            role_count: 1,
            batch_size,
        };

        Ok((Some(bundle), timing))
    }
}

fn filter_placeholder_nodes(neighborhood: NodeNeighborhood) -> NodeNeighborhood {
    let placeholder_ids: BTreeSet<String> = neighborhood
        .neighbors
        .iter()
        .filter(|node| is_placeholder_projection_node(node))
        .map(|node| node.node_id.clone())
        .collect();

    if placeholder_ids.is_empty() {
        return neighborhood;
    }

    NodeNeighborhood {
        root: neighborhood.root,
        neighbors: neighborhood
            .neighbors
            .into_iter()
            .filter(|node| !placeholder_ids.contains(&node.node_id))
            .collect(),
        relations: neighborhood
            .relations
            .into_iter()
            .filter(|relation| {
                !placeholder_ids.contains(&relation.source_node_id)
                    && !placeholder_ids.contains(&relation.target_node_id)
            })
            .collect(),
    }
}

fn is_placeholder_projection_node(node: &NodeProjection) -> bool {
    node.node_kind == "placeholder"
        || node.labels.iter().any(|label| label == "placeholder")
        || node
            .properties
            .get("placeholder")
            .is_some_and(|value| value == "true")
}

async fn load_node_details<D>(
    detail_reader: &D,
    neighborhood: &NodeNeighborhood,
) -> Result<Vec<BundleNodeDetail>, kmp_domain::PortError>
where
    D: NodeDetailReader + Send + Sync,
{
    load_node_details_for_ids(
        detail_reader,
        std::iter::once(neighborhood.root.node_id.clone())
            .chain(
                neighborhood
                    .neighbors
                    .iter()
                    .map(|node| node.node_id.clone()),
            )
            .collect::<Vec<_>>(),
    )
    .await
}

async fn load_node_details_for_ids<D, I>(
    detail_reader: &D,
    node_ids: I,
) -> Result<Vec<BundleNodeDetail>, kmp_domain::PortError>
where
    D: NodeDetailReader + Send + Sync,
    I: IntoIterator<Item = String>,
{
    let mut seen = BTreeSet::new();
    let unique_ids: Vec<String> = node_ids
        .into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect();

    let batch_results = detail_reader.load_node_details_batch(unique_ids).await?;

    Ok(batch_results
        .into_iter()
        .flatten()
        .map(|detail| BundleNodeDetail::from_projection(&detail))
        .collect())
}

fn build_bundle(
    root_node_id: &str,
    role: &str,
    generator_version: &str,
    neighborhood: NodeNeighborhood,
    node_details: Vec<BundleNodeDetail>,
) -> Result<KmpBundle, ApplicationError> {
    let root_node_id = CaseId::new(root_node_id)?;
    let role = Role::new(role)?;

    Ok(KmpBundle::new(
        root_node_id,
        role,
        BundleNode::from_projection(&neighborhood.root),
        neighborhood
            .neighbors
            .iter()
            .map(BundleNode::from_projection)
            .collect(),
        neighborhood
            .relations
            .iter()
            .map(BundleRelationship::from_projection)
            .collect(),
        node_details,
        BundleMetadata::initial(generator_version),
    )?)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use kmp_domain::{
        ContextPathNeighborhood, NodeDetailProjection, NodeNeighborhood, NodeProjection,
        NodeRelationProjection, PortError, RelationExplanation, RelationSemanticClass,
    };
    use tokio::sync::Mutex;

    use super::NodeCentricProjectionReader;
    use crate::queries::DEFAULT_NATIVE_GRAPH_TRAVERSAL_DEPTH;

    struct StubGraphReader;

    impl kmp_domain::GraphNeighborhoodReader for StubGraphReader {
        async fn load_neighborhood(
            &self,
            _root_node_id: &str,
            _depth: u32,
        ) -> Result<Option<NodeNeighborhood>, PortError> {
            Ok(Some(NodeNeighborhood {
                root: NodeProjection {
                    node_id: "node-root".to_string(),
                    node_kind: "case".to_string(),
                    title: "Root".to_string(),
                    summary: "Root summary".to_string(),
                    status: "ACTIVE".to_string(),
                    labels: vec!["ProjectionNode".to_string()],
                    properties: BTreeMap::new(),
                    provenance: None,
                },
                neighbors: vec![NodeProjection {
                    node_id: "node-1".to_string(),
                    node_kind: "decision".to_string(),
                    title: "Neighbor".to_string(),
                    summary: "Neighbor summary".to_string(),
                    status: "ACTIVE".to_string(),
                    labels: vec!["ProjectionNode".to_string()],
                    properties: BTreeMap::new(),
                    provenance: None,
                }],
                relations: vec![NodeRelationProjection {
                    source_node_id: "node-root".to_string(),
                    target_node_id: "node-1".to_string(),
                    relation_type: "RELATES_TO".to_string(),
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
                (root_node_id == "node-root" && target_node_id == "node-1").then_some(
                    ContextPathNeighborhood {
                        root: NodeProjection {
                            node_id: "node-root".to_string(),
                            node_kind: "case".to_string(),
                            title: "Root".to_string(),
                            summary: "Root summary".to_string(),
                            status: "ACTIVE".to_string(),
                            labels: vec!["ProjectionNode".to_string()],
                            properties: BTreeMap::new(),
                            provenance: None,
                        },
                        neighbors: vec![
                            NodeProjection {
                                node_id: "node-1".to_string(),
                                node_kind: "decision".to_string(),
                                title: "Neighbor".to_string(),
                                summary: "Neighbor summary".to_string(),
                                status: "ACTIVE".to_string(),
                                labels: vec!["ProjectionNode".to_string()],
                                properties: BTreeMap::new(),
                                provenance: None,
                            },
                            NodeProjection {
                                node_id: "node-2".to_string(),
                                node_kind: "artifact".to_string(),
                                title: "Leaf".to_string(),
                                summary: "Leaf summary".to_string(),
                                status: "READY".to_string(),
                                labels: vec!["ProjectionNode".to_string()],
                                properties: BTreeMap::new(),
                                provenance: None,
                            },
                        ],
                        relations: vec![
                            NodeRelationProjection {
                                source_node_id: "node-root".to_string(),
                                target_node_id: "node-1".to_string(),
                                relation_type: "RELATES_TO".to_string(),
                                explanation: structural_explanation(),
                            },
                            NodeRelationProjection {
                                source_node_id: "node-1".to_string(),
                                target_node_id: "node-2".to_string(),
                                relation_type: "HAS_ARTIFACT".to_string(),
                                explanation: structural_explanation(),
                            },
                        ],
                        path_node_ids: vec!["node-root".to_string(), "node-1".to_string()],
                    },
                ),
            )
        }
    }

    struct StubDetailReader;

    impl kmp_domain::NodeDetailReader for StubDetailReader {
        async fn load_node_detail(
            &self,
            node_id: &str,
        ) -> Result<Option<NodeDetailProjection>, PortError> {
            Ok((node_id == "node-root").then(|| NodeDetailProjection {
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

    #[tokio::test]
    async fn load_bundle_returns_graph_native_bundle() {
        let reader = NodeCentricProjectionReader::new(StubGraphReader, StubDetailReader);
        let (bundle, _timing) = reader
            .load_bundle("node-root", "developer", "0.1.0")
            .await
            .expect("bundle load should succeed");
        let bundle = bundle.expect("bundle should exist");

        assert_eq!(bundle.root_node().node_id(), "node-root");
        assert_eq!(bundle.neighbor_nodes().len(), 1);
        assert_eq!(bundle.relationships().len(), 1);
        assert_eq!(bundle.node_details().len(), 1);
    }

    struct PlaceholderRootGraphReader;

    impl kmp_domain::GraphNeighborhoodReader for PlaceholderRootGraphReader {
        async fn load_neighborhood(
            &self,
            _root_node_id: &str,
            _depth: u32,
        ) -> Result<Option<NodeNeighborhood>, PortError> {
            Ok(Some(NodeNeighborhood {
                root: NodeProjection {
                    node_id: "node-root".to_string(),
                    node_kind: "placeholder".to_string(),
                    title: "[unmaterialized node]".to_string(),
                    summary: "Referenced by relation before node materialization".to_string(),
                    status: "UNMATERIALIZED".to_string(),
                    labels: vec!["placeholder".to_string()],
                    properties: BTreeMap::from([("placeholder".to_string(), "true".to_string())]),
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

    #[tokio::test]
    async fn load_bundle_returns_none_for_placeholder_root() {
        let reader = NodeCentricProjectionReader::new(PlaceholderRootGraphReader, StubDetailReader);
        let (bundle, _timing) = reader
            .load_bundle("node-root", "developer", "0.1.0")
            .await
            .expect("bundle load should succeed");

        assert!(bundle.is_none());
    }

    struct PlaceholderNeighborGraphReader;

    impl kmp_domain::GraphNeighborhoodReader for PlaceholderNeighborGraphReader {
        async fn load_neighborhood(
            &self,
            _root_node_id: &str,
            _depth: u32,
        ) -> Result<Option<NodeNeighborhood>, PortError> {
            Ok(Some(NodeNeighborhood {
                root: NodeProjection {
                    node_id: "node-root".to_string(),
                    node_kind: "incident".to_string(),
                    title: "Incident".to_string(),
                    summary: "Root summary".to_string(),
                    status: "ACTIVE".to_string(),
                    labels: vec!["incident".to_string()],
                    properties: BTreeMap::new(),
                    provenance: None,
                },
                neighbors: vec![
                    NodeProjection {
                        node_id: "node-real".to_string(),
                        node_kind: "decision".to_string(),
                        title: "Real node".to_string(),
                        summary: "Real summary".to_string(),
                        status: "ACTIVE".to_string(),
                        labels: vec!["decision".to_string()],
                        properties: BTreeMap::new(),
                        provenance: None,
                    },
                    NodeProjection {
                        node_id: "node-placeholder".to_string(),
                        node_kind: "placeholder".to_string(),
                        title: "[unmaterialized node]".to_string(),
                        summary: "Referenced by relation before node materialization".to_string(),
                        status: "UNMATERIALIZED".to_string(),
                        labels: vec!["placeholder".to_string()],
                        properties: BTreeMap::from([(
                            "placeholder".to_string(),
                            "true".to_string(),
                        )]),
                        provenance: None,
                    },
                ],
                relations: vec![
                    NodeRelationProjection {
                        source_node_id: "node-root".to_string(),
                        target_node_id: "node-real".to_string(),
                        relation_type: "RELATES_TO".to_string(),
                        explanation: structural_explanation(),
                    },
                    NodeRelationProjection {
                        source_node_id: "node-root".to_string(),
                        target_node_id: "node-placeholder".to_string(),
                        relation_type: "RELATES_TO".to_string(),
                        explanation: structural_explanation(),
                    },
                ],
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

    struct PlaceholderDetailReader;

    impl kmp_domain::NodeDetailReader for PlaceholderDetailReader {
        async fn load_node_detail(
            &self,
            node_id: &str,
        ) -> Result<Option<NodeDetailProjection>, PortError> {
            Ok(match node_id {
                "node-root" => Some(NodeDetailProjection {
                    node_id: node_id.to_string(),
                    detail: "Expanded detail".to_string(),
                    content_hash: "hash-root".to_string(),
                    revision: 1,
                }),
                "node-real" => Some(NodeDetailProjection {
                    node_id: node_id.to_string(),
                    detail: "Real detail".to_string(),
                    content_hash: "hash-real".to_string(),
                    revision: 1,
                }),
                "node-placeholder" => Some(NodeDetailProjection {
                    node_id: node_id.to_string(),
                    detail: "Placeholder detail".to_string(),
                    content_hash: "hash-placeholder".to_string(),
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

    #[tokio::test]
    async fn load_bundle_filters_placeholder_neighbors_relations_and_details() {
        let reader = NodeCentricProjectionReader::new(
            PlaceholderNeighborGraphReader,
            PlaceholderDetailReader,
        );
        let (bundle, _timing) = reader
            .load_bundle("node-root", "developer", "0.1.0")
            .await
            .expect("bundle load should succeed");
        let bundle = bundle.expect("bundle should exist");

        assert_eq!(bundle.neighbor_nodes().len(), 1);
        assert_eq!(bundle.neighbor_nodes()[0].node_id(), "node-real");
        assert_eq!(bundle.relationships().len(), 1);
        assert_eq!(bundle.relationships()[0].target_node_id(), "node-real");
        assert_eq!(
            bundle
                .node_details()
                .iter()
                .map(|detail| detail.node_id())
                .collect::<Vec<_>>(),
            vec!["node-root", "node-real"]
        );
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
            Ok(None)
        }

        async fn load_context_path(
            &self,
            _root_node_id: &str,
            _target_node_id: &str,
            subtree_depth: u32,
        ) -> Result<Option<ContextPathNeighborhood>, PortError> {
            self.depths.lock().await.push(subtree_depth);
            Ok(None)
        }
    }

    #[tokio::test]
    async fn load_bundle_uses_default_graph_traversal_depth() {
        let depths = Arc::new(Mutex::new(Vec::new()));
        let reader = NodeCentricProjectionReader::new(
            RecordingGraphReader {
                depths: Arc::clone(&depths),
            },
            StubDetailReader,
        );

        let (bundle, _timing) = reader
            .load_bundle("node-root", "developer", "0.1.0")
            .await
            .expect("bundle load should succeed");

        assert!(bundle.is_none());
        assert_eq!(
            &*depths.lock().await,
            &[DEFAULT_NATIVE_GRAPH_TRAVERSAL_DEPTH]
        );
    }

    #[tokio::test]
    async fn load_context_path_bundle_allows_zero_subtree_depth() {
        let depths = Arc::new(Mutex::new(Vec::new()));
        let reader = NodeCentricProjectionReader::new(
            RecordingGraphReader {
                depths: Arc::clone(&depths),
            },
            StubDetailReader,
        );

        let (bundle, _timing) = reader
            .load_context_path_bundle_with_depth("node-root", "node-1", "developer", "0.1.0", 0)
            .await
            .expect("path bundle load should not reject zero subtree depth");

        assert!(bundle.is_none());
        assert_eq!(&*depths.lock().await, &[0]);
    }

    #[tokio::test]
    async fn load_context_path_bundle_only_includes_details_for_path_nodes() {
        let reader = NodeCentricProjectionReader::new(StubGraphReader, StubDetailReader);
        let (bundle, _timing) = reader
            .load_context_path_bundle_with_depth("node-root", "node-1", "developer", "0.1.0", 4)
            .await
            .expect("path bundle load should succeed");
        let bundle = bundle.expect("bundle should exist");

        assert_eq!(bundle.root_node().node_id(), "node-root");
        assert_eq!(bundle.neighbor_nodes().len(), 2);
        assert_eq!(bundle.relationships().len(), 2);
        assert_eq!(
            bundle
                .node_details()
                .iter()
                .map(|detail| detail.node_id())
                .collect::<Vec<_>>(),
            vec!["node-root"]
        );
    }

    fn structural_explanation() -> RelationExplanation {
        RelationExplanation::new(RelationSemanticClass::Structural)
    }

    #[tokio::test]
    async fn load_bundles_for_roles_returns_none_when_node_missing() {
        let reader = NodeCentricProjectionReader::new(
            RecordingGraphReader {
                depths: Arc::new(Mutex::new(Vec::new())),
            },
            StubDetailReader,
        );

        let (result, _timing) = reader
            .load_bundles_for_roles("nonexistent", &["dev".to_string()], "0.1.0", 3)
            .await
            .expect("should not error");

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn load_bundles_for_roles_single_role_matches_load_bundle() {
        let reader = NodeCentricProjectionReader::new(StubGraphReader, StubDetailReader);

        let (single, _timing) = reader
            .load_bundle("node-root", "developer", "0.1.0")
            .await
            .expect("single load should succeed");
        let single = single.expect("bundle should exist");

        let (multi, _timing) = reader
            .load_bundles_for_roles(
                "node-root",
                &["developer".to_string()],
                "0.1.0",
                DEFAULT_NATIVE_GRAPH_TRAVERSAL_DEPTH,
            )
            .await
            .expect("multi load should succeed");
        let multi = multi.expect("bundles should exist");

        assert_eq!(multi.len(), 1);
        assert_eq!(multi[0].root_node().node_id(), single.root_node().node_id());
        assert_eq!(
            multi[0].neighbor_nodes().len(),
            single.neighbor_nodes().len()
        );
        assert_eq!(multi[0].relationships().len(), single.relationships().len());
        assert_eq!(multi[0].node_details().len(), single.node_details().len());
    }

    #[tokio::test]
    async fn load_bundles_for_roles_produces_correct_count() {
        let reader = NodeCentricProjectionReader::new(
            StubGraphReader,
            CountingDetailReader {
                call_count: Arc::new(Mutex::new(0)),
            },
        );

        let (bundles, _timing) = reader
            .load_bundles_for_roles(
                "node-root",
                &["dev".to_string(), "reviewer".to_string(), "ops".to_string()],
                "0.1.0",
                DEFAULT_NATIVE_GRAPH_TRAVERSAL_DEPTH,
            )
            .await
            .expect("should succeed");
        let bundles = bundles.expect("bundles should exist");

        assert_eq!(bundles.len(), 3);
    }

    struct CountingDetailReader {
        call_count: Arc<Mutex<u32>>,
    }

    impl kmp_domain::NodeDetailReader for CountingDetailReader {
        async fn load_node_detail(
            &self,
            node_id: &str,
        ) -> Result<Option<NodeDetailProjection>, PortError> {
            Ok(Some(NodeDetailProjection {
                node_id: node_id.to_string(),
                detail: "detail".to_string(),
                content_hash: "hash".to_string(),
                revision: 1,
            }))
        }

        async fn load_node_details_batch(
            &self,
            node_ids: Vec<String>,
        ) -> Result<Vec<Option<NodeDetailProjection>>, PortError> {
            *self.call_count.lock().await += 1;
            let mut results = Vec::with_capacity(node_ids.len());
            for node_id in &node_ids {
                results.push(self.load_node_detail(node_id).await?);
            }
            Ok(results)
        }
    }

    #[tokio::test]
    async fn batch_detail_reader_called_once_for_multi_role() {
        let call_count = Arc::new(Mutex::new(0u32));
        let reader = NodeCentricProjectionReader::new(
            StubGraphReader,
            CountingDetailReader {
                call_count: Arc::clone(&call_count),
            },
        );

        let (bundles, timing) = reader
            .load_bundles_for_roles(
                "node-root",
                &["dev".to_string(), "reviewer".to_string()],
                "0.1.0",
                DEFAULT_NATIVE_GRAPH_TRAVERSAL_DEPTH,
            )
            .await
            .expect("should succeed");
        let bundles = bundles.expect("bundles should exist");

        assert_eq!(bundles.len(), 2);
        // Batch is called once (shared load), not once per role.
        assert_eq!(*call_count.lock().await, 1);
        assert_eq!(timing.role_count, 2);
        assert!(timing.batch_size > 0);
    }

    struct NoDetailReader;

    impl kmp_domain::NodeDetailReader for NoDetailReader {
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
            Ok(vec![None; node_ids.len()])
        }
    }

    #[tokio::test]
    async fn load_bundle_with_no_details_returns_empty_details() {
        let reader = NodeCentricProjectionReader::new(StubGraphReader, NoDetailReader);

        let (bundle, _timing) = reader
            .load_bundle("node-root", "developer", "0.1.0")
            .await
            .expect("should succeed");
        let bundle = bundle.expect("bundle should exist");

        assert_eq!(bundle.root_node().node_id(), "node-root");
        assert_eq!(bundle.neighbor_nodes().len(), 1);
        assert!(bundle.node_details().is_empty());
    }
}
