use super::*;

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
