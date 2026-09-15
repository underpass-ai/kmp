use super::*;

struct StoredAboutReader;

impl GraphNeighborhoodReader for StoredAboutReader {
    async fn load_neighborhood(
        &self,
        root_node_id: &str,
        _depth: u32,
    ) -> Result<Option<NodeNeighborhood>, PortError> {
        Ok(
            (root_node_id == "question:830ce83f").then_some(NodeNeighborhood {
                root: temporal_projection(
                    "question:830ce83f",
                    "memory_anchor",
                    "Stored relocation context",
                ),
                neighbors: vec![
                    temporal_projection(
                        "label:v1:question%3A830ce83f:conversation:conversation",
                        "memory_dimension",
                        "Stored conversation",
                    ),
                    temporal_projection(
                        "question:830ce83f:entry:claim:rachel-denver",
                        "claim",
                        "Rachel said she was moving to Denver.",
                    ),
                ],
                relations: vec![temporal_contains_entry(
                    "label:v1:question%3A830ce83f:conversation:conversation",
                    "question:830ce83f:entry:claim:rachel-denver",
                    "conversation",
                    1,
                    Some(sort_time(100)),
                    None,
                    None,
                )],
            }),
        )
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

impl MemoryAboutIndexReader for StoredAboutReader {
    async fn list_memory_abouts(&self) -> Result<Vec<String>, PortError> {
        Ok(vec!["question:830ce83f".to_string()])
    }
}

impl NodeRelationshipReader for StoredAboutReader {
    async fn load_node_relationships(
        &self,
        node_id: &str,
    ) -> Result<Option<NodeRelationships>, PortError> {
        Ok((node_id == "question:830ce83f").then_some(NodeRelationships::default()))
    }
}

#[tokio::test]
async fn grpc_service_requires_neighborhood_acknowledgement_before_materializing_rich_links() {
    let store = Arc::new(InMemoryContextEventStore::new());
    let service = memory_service_with_store(
        EmptyGraphNeighborhoodReader,
        EmptyNodeDetailReader,
        store.clone(),
    );
    let mut request = valid_memory_ingest_request(false);
    request.neighborhood_review = Some(String::new());
    let memory = request.memory.as_mut().expect("native ingest response");
    let mut second = memory.entries[0].clone();
    second.id = "question:830ce83f:claim:second-report".into();
    second.text = "A second report confirms Rachel's move.".into();
    memory.relations.push(MemoryRelation {
        source_ref: second.id.clone(),
        target_ref: memory.entries[0].id.clone(),
        rel: "supports".into(),
        semantic_class: MemorySemanticClass::Evidential as i32,
        confidence: MemoryConfidence::High as i32,
        why: "The independent report confirms the destination.".into(),
        evidence: "Second report: Rachel moved to Denver.".into(),
        ..Default::default()
    });
    memory.entries.push(second);
    let pending = service
        .ingest(Request::new(request.clone()))
        .await
        .expect("native ingest response")
        .into_inner();
    let pending = kmp_proto::v1beta1::IngestResponse::decode(pending.encode_to_vec().as_slice())
        .expect("native ingest response");
    let view = pending
        .neighborhood
        .expect("typed review crosses the protobuf boundary");
    assert_eq!(view.items.len(), 2);
    assert!(view.items.iter().all(|item| item.state == "proposed"));
    assert!(
        view.stored_abouts.is_empty(),
        "a new proposal has no stored owner to expand after protobuf round-trip"
    );
    let result = pending.memory.expect("native ingest response");
    assert_eq!(result.accepted.expect("native ingest response").entries, 0);
    assert!(result.receipt_ref.is_none());
    assert!(result.clocks.is_none());
    assert_eq!(
        store
            .current_revision(&request.about, "memory")
            .await
            .expect("native ingest response"),
        0
    );
    request.neighborhood_review = Some(view.token);
    let accepted = service
        .ingest(Request::new(request))
        .await
        .expect("native ingest response")
        .into_inner();
    assert!(accepted.neighborhood.is_none());
    assert!(
        accepted
            .memory
            .expect("native ingest response")
            .read_after_write_ready
    );
    assert_eq!(
        store
            .current_revision("question:830ce83f", "memory")
            .await
            .expect("native ingest response"),
        1
    );
}

#[tokio::test]
async fn grpc_review_carries_an_existing_owner_through_the_wire() {
    let service = memory_service(StoredAboutReader, EmptyNodeDetailReader);
    let mut request = valid_memory_ingest_request(false);
    request.neighborhood_review = Some(String::new());
    let memory = request.memory.as_mut().expect("native ingest request");
    memory.relations.push(MemoryRelation {
        source_ref: memory.entries[0].id.clone(),
        target_ref: "question:830ce83f:entry:claim:rachel-denver".into(),
        rel: "supports".into(),
        semantic_class: MemorySemanticClass::Evidential as i32,
        confidence: MemoryConfidence::High as i32,
        why: "The new report supports the stored relocation claim.".into(),
        evidence: "The new report repeats Rachel's stated destination.".into(),
        ..Default::default()
    });

    let pending = service
        .ingest(Request::new(request.clone()))
        .await
        .expect("native ingest response")
        .into_inner();
    let pending = kmp_proto::v1beta1::IngestResponse::decode(pending.encode_to_vec().as_slice())
        .expect("typed review crosses the protobuf boundary");
    let view = pending
        .neighborhood
        .expect("a rich link to stored context needs review");

    assert_eq!(view.stored_abouts, vec![request.about]);
}
