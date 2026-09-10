//! Real service/application traversal; the repository fixture owns only source data.
use super::*;

#[path = "temporal_entry_selection_tests.rs"]
mod entry_selection_tests;

struct EvidencedTemporalReader;

impl GraphNeighborhoodReader for EvidencedTemporalReader {
    async fn load_neighborhood(
        &self,
        root: &str,
        depth: u32,
    ) -> Result<Option<NodeNeighborhood>, PortError> {
        let mut neighborhood = TemporalGraphNeighborhoodReader
            .load_neighborhood(root, depth)
            .await?;
        if let Some(value) = &mut neighborhood {
            for relation in &mut value.relations {
                if relation.relation_type == "supersedes" {
                    relation.explanation = relation.explanation.clone().with_evidence(
                        "The later statement explicitly replaces Denver with Austin.",
                    );
                }
            }
        }
        Ok(neighborhood)
    }

    async fn load_context_path(
        &self,
        root: &str,
        role: &str,
        depth: u32,
    ) -> Result<Option<ContextPathNeighborhood>, PortError> {
        TemporalGraphNeighborhoodReader
            .load_context_path(root, role, depth)
            .await
    }
}

impl MemoryAboutIndexReader for EvidencedTemporalReader {
    async fn list_memory_abouts(&self) -> Result<Vec<String>, PortError> {
        TemporalGraphNeighborhoodReader.list_memory_abouts().await
    }
}

impl NodeRelationshipReader for EvidencedTemporalReader {
    async fn load_node_relationships(
        &self,
        node: &str,
    ) -> Result<Option<NodeRelationships>, PortError> {
        TemporalGraphNeighborhoodReader
            .load_node_relationships(node)
            .await
    }
}

#[tokio::test]
async fn native_temporal_responses_keep_dependency_records_through_the_real_grpc_service() {
    let service = memory_service(EvidencedTemporalReader, EmptyNodeDetailReader);
    let mut request = temporal_move_request(
        Some(ProtoTemporalCursor {
            time: Some(ts(106)),
            ..Default::default()
        }),
        ProtoDimensionSelection::default(),
    );
    request
        .limit
        .as_mut()
        .expect("expected temporal service fixture")
        .entries = 1;
    request.include = Some(TemporalInclude {
        dependencies: true,
        ..Default::default()
    });
    let goto = service
        .goto(Request::new(GotoRequest {
            about: request.about.clone(),
            cursor: request.cursor.clone(),
            dimensions: request.dimensions.clone(),
            limit: request.limit,
            include: request.include,
            budget: request.budget,
            ..Default::default()
        }))
        .await
        .expect("real goto")
        .into_inner();
    assert_eq!(goto.entries.len(), 1);
    assert_eq!(goto.entries[0].r#ref, "claim:rachel-austin");
    assert_eq!(
        goto.dependency_groups[0].member_refs,
        ["claim:rachel-austin", "claim:rachel-denver"]
    );
    assert_eq!(
        goto.dependency_entries[0].text,
        "Rachel said she was moving to Denver."
    );
    assert!(!goto.dependency_entries[0].coordinates.is_empty());
    assert!(
        goto.proof
            .as_ref()
            .expect("expected temporal service fixture")
            .path
            .iter()
            .any(|edge| edge.rel == "supersedes")
    );
    let rewind = service
        .rewind(Request::new(RewindRequest {
            about: request.about.clone(),
            cursor: request.cursor.clone(),
            limit: request.limit,
            include: request.include,
            ..Default::default()
        }))
        .await
        .expect("real rewind")
        .into_inner();
    let forward = service
        .forward(Request::new(ForwardRequest {
            about: request.about.clone(),
            cursor: Some(ProtoTemporalCursor {
                time: Some(ts(101)),
                ..Default::default()
            }),
            limit: request.limit,
            include: request.include,
            ..Default::default()
        }))
        .await
        .expect("real forward")
        .into_inner();
    let near = service
        .near(Request::new(NearRequest {
            about: request.about,
            around: request.cursor,
            limit: request.limit,
            include: request.include,
            window: Some(ProtoTemporalWindow {
                before_entries: 1,
                after_entries: 0,
            }),
            ..Default::default()
        }))
        .await
        .expect("real near")
        .into_inner();
    for (groups, entries) in [
        (rewind.dependency_groups, rewind.dependency_entries),
        (forward.dependency_groups, forward.dependency_entries),
        (near.dependency_groups, near.dependency_entries),
    ] {
        assert_eq!(groups, goto.dependency_groups);
        assert_eq!(entries, goto.dependency_entries);
    }
}
