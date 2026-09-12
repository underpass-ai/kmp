//! Real wire dispatch, same SQLite snapshot semantics as the embedded service.
use kmp_domain::{
    NodeDetailProjection, NodeProjection, NodeRelationProjection, ProjectionMutation,
    ProjectionWriter, RelationExplanation, RelationSemanticClass,
};
use kmp_embedded::EmbeddedKernel;
use kmp_proto::v1beta1::{
    PageRequest, TraceReferenceEndpoint, TraceRelationStep, TraceRequest, TraceSearchOptions,
    TraceSeekOptions, TraceSeekRole, TraceWitnessGroup,
    kernel_memory_service_client::KernelMemoryServiceClient,
    kernel_memory_service_server::KernelMemoryServiceServer,
};
use kmp_transport_grpc::MemoryGrpcService;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;

#[tokio::test]
async fn seek_over_grpc_matches_embedded_and_pages_candidate_groups()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    check(false, false, false).await
}

#[tokio::test]
async fn context_sequences_over_grpc_match_embedded_and_preserve_witness_positions()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    check(true, false, false).await
}

#[tokio::test]
async fn relation_endpoints_over_grpc_match_embedded_and_survive_paging()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    check(true, true, false).await
}

#[tokio::test]
async fn seek_materializes_shared_sources_over_grpc()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    check(false, false, true).await
}

#[tokio::test]
async fn context_materializes_sources_without_promoting_review_state()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    check(true, false, true).await
}

#[tokio::test]
async fn joined_endpoints_share_sources_and_keep_all_proof_pages()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    check(true, true, true).await
}

#[tokio::test]
async fn named_seek_expansion_preserves_manifest_budget_and_refs_across_metadata_pages()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    check_admission(true, true, true, true).await
}

async fn check(
    context: bool,
    endpoints: bool,
    proof: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    check_admission(context, endpoints, proof, false).await
}

async fn check_admission(
    context: bool,
    endpoints: bool,
    proof: bool,
    admission: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let dir = tempfile::tempdir()?;
    let kernel = EmbeddedKernel::open(dir.path())?;
    let about = "project:seek-wire";
    let seed = format!("{about}:entry:observation:seed");
    let anchor = if context {
        format!("{about}:entry:observation:anchor")
    } else {
        seed.clone()
    };
    let witness = format!("{about}:entry:observation:report");
    let mut mutations = vec![];
    for id in [&seed, &anchor, &witness] {
        mutations.push(ProjectionMutation::UpsertNode(NodeProjection {
            node_id: id.clone(),
            node_kind: "observation".into(),
            title: id.clone(),
            summary: id.clone(),
            status: "ACTIVE".into(),
            labels: vec!["entry".into()],
            properties: [("memory_about".into(), about.into())].into(),
            provenance: None,
        }));
        let dimension = kmp_domain::MemoryDimensionIdentity::new(about, "event", "E1")?.node_id();
        mutations.push(ProjectionMutation::UpsertNodeRelation(Box::new(
            NodeRelationProjection {
                source_node_id: dimension.clone(),
                target_node_id: id.clone(),
                relation_type: "contains_entry".into(),
                explanation: RelationExplanation::new(RelationSemanticClass::Structural)
                    .with_dimension("event")
                    .with_scope_id(dimension)
                    .with_observed_at("2026-09-01T00:00:00Z"),
            },
        )));
    }
    mutations.push(ProjectionMutation::UpsertNodeRelation(Box::new(
        NodeRelationProjection {
            source_node_id: anchor.clone(),
            target_node_id: witness.clone(),
            relation_type: "verified_by".into(),
            explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
                .with_rationale("The report explicitly verifies this execution.")
                .with_evidence("Report: execution verified.")
                .with_observed_at("2026-09-01T00:00:00Z"),
        },
    )));
    if context {
        mutations.push(ProjectionMutation::UpsertNodeRelation(Box::new(
            NodeRelationProjection {
                source_node_id: seed.clone(),
                target_node_id: anchor.clone(),
                relation_type: "uses_background".into(),
                explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
                    .with_rationale("The seed refers to this contextual execution.")
                    .with_evidence("Seed: related execution recorded.")
                    .with_observed_at("2026-09-01T00:00:00Z"),
            },
        )));
    }
    let source = format!("evidence:{about}:shared");
    if proof {
        mutations.push(ProjectionMutation::UpsertNode(NodeProjection {
            node_id: source.clone(),
            node_kind: "memory_evidence".into(),
            title: "signed source".into(),
            summary: "summary".into(),
            status: "ACTIVE".into(),
            labels: vec![],
            properties: [
                ("memory_about".into(), about.into()),
                ("payload_source".into(), "fixture:signed-source".into()),
                ("payload_time".into(), "2026-09-01T00:00:00Z".into()),
            ]
            .into(),
            provenance: None,
        }));
        let refs: std::collections::BTreeSet<_> = [&seed, &anchor, &witness].into_iter().collect();
        for id in refs {
            mutations.push(ProjectionMutation::UpsertNodeDetail(NodeDetailProjection {
                node_id: id.clone(),
                detail: format!("Exact full body of {id}"),
                content_hash: format!("hash:{id}"),
                revision: 2,
            }));
            mutations.push(ProjectionMutation::UpsertNodeRelation(Box::new(
                NodeRelationProjection {
                    source_node_id: source.clone(),
                    target_node_id: id.clone(),
                    relation_type: "supports".into(),
                    explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
                        .with_rationale("Signed source backs this entry")
                        .with_evidence("Exact signed source π")
                        .with_observed_at("2026-09-01T00:00:00Z"),
                },
            )));
        }
        mutations.push(ProjectionMutation::UpsertNodeDetail(NodeDetailProjection {
            node_id: source.clone(),
            detail: "Exact signed source π".into(),
            content_hash: "source-hash".into(),
            revision: 3,
        }));
    }
    kernel.store().apply_mutations(mutations).await?;
    let mut request = TraceRequest {
        about: about.into(),
        from: seed,
        search: Some(TraceSearchOptions {
            proof,
            seek: Some(TraceSeekOptions {
                roles: vec![TraceSeekRole {
                    name: "verification".into(),
                    context,
                    relation: Some(TraceRelationStep {
                        rel: "verified_by".into(),
                        direction: "outgoing".into(),
                    }),
                    ..Default::default()
                }],
                same_labels: vec!["event".into()],
                ..Default::default()
            }),
            ..Default::default()
        }),
        page: Some(PageRequest {
            entries: 1,
            ..Default::default()
        }),
        ..Default::default()
    };
    if endpoints {
        let seek = request
            .search
            .as_mut()
            .expect("valid test fixture")
            .seek
            .as_mut()
            .expect("valid test fixture");
        seek.roles.push(TraceSeekRole {
            name: "reverse".into(),
            context: true,
            relation: Some(TraceRelationStep {
                rel: "verified_by".into(),
                direction: "incoming".into(),
            }),
            ..Default::default()
        });
        seek.same_ref.push(TraceWitnessGroup {
            endpoints: vec![
                TraceReferenceEndpoint {
                    role: "verification".into(),
                    anchor: true,
                },
                TraceReferenceEndpoint {
                    role: "reverse".into(),
                    anchor: false,
                },
            ],
        });
    }
    let native = kmp_proto_mapping::v1beta1::evidence_seek_request_from_proto(&request)
        .expect("valid")
        .expect("seek");
    let result = kernel.service().evidence_paths(native.clone()).await?;
    let seek = request
        .search
        .as_ref()
        .expect("search")
        .seek
        .as_ref()
        .expect("seek");
    let expected = kmp_proto_mapping::v1beta1::evidence_seek_response_from_result(
        result,
        &native,
        seek,
        kmp_application::TracePageRequest {
            entries: Some(1),
            cursor: None,
        },
    );
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(KernelMemoryServiceServer::new(MemoryGrpcService::new(
                kernel.service(),
            )))
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async {
                let _ = stopped.await;
            }),
    );
    let mut client = KernelMemoryServiceClient::connect(endpoint).await?;
    if admission {
        check_named_pages(&mut client, request.clone()).await?;
        stop.send(()).expect("stop");
        server.await??;
        return Ok(());
    }
    let first = client.trace(request.clone()).await?.into_inner();
    assert_eq!(first, expected);
    let selection = first.seek.as_ref().expect("seek");
    assert_eq!(
        selection.status,
        if context {
            "review_required"
        } else {
            "compatible"
        }
    );
    assert_eq!(selection.context_discovery, context);
    assert_eq!(selection.declared_obligations_complete, !context);
    let mut page = first.clone();
    let mut candidates = first.candidates.clone();
    let mut groups = first.groups.clone();
    let mut objects = first.objects.clone();
    let mut supports = first.supports.clone();
    let mut gaps = first.gaps.clone();
    while page.page.as_ref().expect("page").has_more {
        let mut next = request.clone();
        next.page.as_mut().expect("page").cursor = page.page.expect("page").next_cursor;
        page = client.trace(next).await?.into_inner();
        assert_eq!(page.selection_fingerprint, first.selection_fingerprint);
        candidates.extend(page.candidates.clone());
        groups.extend(page.groups.clone());
        objects.extend(page.objects.clone());
        supports.extend(page.supports.clone());
        gaps.extend(page.gaps.clone());
    }
    if proof {
        assert_eq!(first.proof.as_ref().expect("proof").complete_groups, [0]);
        assert!(gaps.is_empty());
        assert_eq!(objects.len(), if context { 4 } else { 3 });
        assert!(objects.iter().all(|o| o.has_body));
        let fetched = objects
            .iter()
            .filter(|o| o.object.as_ref().expect("object").r#ref == source)
            .collect::<Vec<_>>();
        assert_eq!(fetched.len(), 1);
        assert_eq!(fetched[0].revision, 3);
        assert_eq!(
            fetched[0].object.as_ref().expect("source").text,
            "Exact signed source π"
        );
        assert_eq!(supports.len(), if context { 3 } else { 2 });
        assert!(supports.iter().all(|e| e.source_ref == source));
        // Only a body changes: the complete selection fingerprint still changes,
        // including when the changed object is absent from the first wire page.
        kernel
            .store()
            .apply_mutations(vec![ProjectionMutation::UpsertNodeDetail(
                NodeDetailProjection {
                    node_id: source,
                    detail: "Changed signed source".into(),
                    content_hash: "new-source-hash".into(),
                    revision: 4,
                },
            )])
            .await?;
        let changed = client.trace(request.clone()).await?.into_inner();
        assert_ne!(changed.selection_fingerprint, first.selection_fingerprint);
    } else {
        assert!(first.proof.is_none());
        assert!(objects.is_empty());
    }
    assert_eq!(candidates[0].witness, witness);
    assert_eq!(candidates[0].anchor, anchor);
    assert_eq!(candidates[0].context_hops, u32::from(context));
    assert_eq!(
        groups[0].candidate_indexes,
        if endpoints { vec![0, 1] } else { vec![0] }
    );
    if endpoints {
        let binding = groups[0]
            .bindings
            .iter()
            .find(|b| b.reference)
            .expect("valid test fixture");
        assert_eq!(binding.values, [anchor]);
        assert_eq!(binding.endpoints.len(), 2);
        assert!(binding.endpoints[0].anchor);
        assert!(!binding.endpoints[1].anchor);
    }
    stop.send(()).expect("stop");
    server.await??;
    Ok(())
}

async fn check_named_pages(
    client: &mut KernelMemoryServiceClient<tonic::transport::Channel>,
    mut request: TraceRequest,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use kmp_proto::v1beta1::TraceBodyRefs;
    use std::collections::BTreeMap;
    const B: u64 = 8192;
    let search = request.search.as_mut().expect("search");
    search.proof_refs = Some(TraceBodyRefs { refs: vec![] });
    search.max_body_record_bytes = B;
    let first = client.trace(request.clone()).await?.into_inner();
    let manifest = first.proof.as_ref().expect("proof").manifest_id.clone();
    assert!(!manifest.is_empty());
    let plan = first.proof.as_ref().expect("proof").expansion_plan.clone();
    assert!(plan.is_some(), "global continuation exists before page cut");
    let mut descriptors = BTreeMap::new();
    let mut page = first;
    let mut pages = 0;
    loop {
        pages += 1;
        assert!(pages < 40, "metadata traversal terminates");
        let proof = page.proof.as_ref().expect("proof");
        assert_eq!(proof.manifest_id, manifest);
        assert_eq!(proof.expansion_plan, plan);
        for object in &page.objects {
            assert_eq!(object.body_state, "not_requested");
            let inspected = object.object.as_ref().expect("object");
            assert!(inspected.text.is_empty());
            assert!(
                descriptors
                    .insert(
                        inspected.r#ref.clone(),
                        object.descriptor.clone().expect("descriptor")
                    )
                    .is_none()
            );
        }
        let info = page.page.as_ref().expect("page");
        if !info.has_more {
            break;
        }
        request.page.as_mut().expect("page").cursor = info.next_cursor.clone();
        page = client.trace(request.clone()).await?.into_inner();
    }
    assert!(pages > 1);
    assert_eq!(descriptors.len(), 4, "all three entries and shared source");
    // Each named body traverses all metadata pages: no page boundary can drop it.
    for (id, descriptor) in &descriptors {
        request.page.as_mut().expect("page").cursor.clear();
        let search = request.search.as_mut().expect("search");
        search.proof_refs = Some(TraceBodyRefs {
            refs: vec![id.clone()],
        });
        search.expect_selection = manifest.clone();
        let mut loaded = vec![];
        for step in 0..40 {
            let response = client.trace(request.clone()).await?.into_inner();
            assert!(response.expansion_refusal.is_none());
            let proof = response.proof.as_ref().expect("proof");
            assert_eq!(proof.manifest_id, manifest);
            assert_eq!(
                proof
                    .delivery
                    .as_ref()
                    .expect("delivery")
                    .admitted_record_bytes,
                descriptor.record_bytes
            );
            assert!(descriptor.record_bytes <= B);
            for object in &response.objects {
                let inspected = object.object.as_ref().expect("object");
                if object.body_state == "loaded" {
                    assert_eq!(&inspected.r#ref, id);
                    assert!(!inspected.text.is_empty());
                    loaded.push(inspected.r#ref.clone());
                } else {
                    assert!(inspected.text.is_empty());
                }
            }
            let info = response.page.as_ref().expect("page");
            if !info.has_more {
                break;
            }
            assert!(step < 39, "named pagination terminates");
            request.page.as_mut().expect("page").cursor = info.next_cursor.clone();
        }
        assert_eq!(loaded.as_slice(), std::slice::from_ref(id));
    }
    request.page.as_mut().expect("page").cursor.clear();
    request.search.as_mut().expect("search").proof_refs = Some(TraceBodyRefs {
        refs: vec!["outside-selection".into()],
    });
    let refused = client.trace(request).await?.into_inner();
    assert!(refused.expansion_refusal.is_some());
    assert!(refused.objects.is_empty());
    Ok(())
}
