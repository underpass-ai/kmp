//! An older selected seed can retain newer admitted proof through the real service.
use super::*;
use kmp_proto::v1beta1::TemporalEntrySelection;

#[tokio::test]
async fn explicit_older_seed_preserves_newer_dependency_for_each_temporal_verb() {
    let service = memory_service(EvidencedTemporalReader, EmptyNodeDetailReader);
    let request = temporal_move_request(None, ProtoDimensionSelection::default());
    let cursor = Some(ProtoTemporalCursor {
        time: Some(ts(106)),
        ..Default::default()
    });
    let selection = Some(TemporalEntrySelection {
        refs: vec!["claim:rachel-denver".into()],
    });
    let include = Some(TemporalInclude {
        dependencies: true,
        ..Default::default()
    });
    let goto = service
        .goto(Request::new(GotoRequest {
            about: request.about.clone(),
            cursor: cursor.clone(),
            entry_selection: selection.clone(),
            include,
            limit: request.limit,
            ..Default::default()
        }))
        .await
        .expect("goto")
        .into_inner();
    assert_eq!(goto.entries.len(), 1);
    assert_eq!(goto.entries[0].r#ref, "claim:rachel-denver");
    assert_eq!(goto.dependency_entries.len(), 1);
    assert_eq!(goto.dependency_entries[0].r#ref, "claim:rachel-austin");
    // Goto bounds proof at 106 and excludes a sequence-only membership.
    // The other moves have frontier proof unless an interval end is supplied.
    assert_eq!(goto.dependency_entries[0].coordinates.len(), 2);
    let unbounded = service
        .rewind(Request::new(RewindRequest {
            about: request.about.clone(),
            cursor: cursor.clone(),
            entry_selection: selection.clone(),
            include,
            limit: request.limit,
            ..Default::default()
        }))
        .await
        .expect("unbounded rewind")
        .into_inner();
    assert_eq!(unbounded.dependency_entries.len(), 1);
    assert_eq!(unbounded.dependency_entries[0].coordinates.len(), 3);
    let extra = &unbounded.dependency_entries[0].coordinates[2];
    assert_eq!(extra.dimension, "benchmark_record");
    assert_eq!(extra.sequence, Some(7));
    assert!(
        extra.occurred_at.is_none()
            && extra.observed_at.is_none()
            && extra.ingested_at.is_none()
            && extra.valid_from.is_none()
            && extra.valid_until.is_none()
    );
    let interval = Some(kmp_proto::v1beta1::TemporalInterval {
        start: None,
        end: Some(ts(107)),
    });
    let rewind = service
        .rewind(Request::new(RewindRequest {
            about: request.about.clone(),
            cursor: cursor.clone(),
            entry_selection: selection.clone(),
            interval: interval.clone(),
            include,
            limit: request.limit,
            ..Default::default()
        }))
        .await
        .expect("rewind")
        .into_inner();
    let forward = service
        .forward(Request::new(ForwardRequest {
            interval: interval.clone(),
            about: request.about.clone(),
            cursor: Some(ProtoTemporalCursor {
                time: Some(ts(99)),
                ..Default::default()
            }),
            entry_selection: selection.clone(),
            include,
            limit: request.limit,
            ..Default::default()
        }))
        .await
        .expect("forward")
        .into_inner();
    let near = service
        .near(Request::new(NearRequest {
            interval,
            about: request.about,
            around: cursor,
            entry_selection: selection,
            include,
            limit: request.limit,
            window: Some(ProtoTemporalWindow {
                before_entries: 1,
                after_entries: 0,
            }),
            ..Default::default()
        }))
        .await
        .expect("near")
        .into_inner();
    for (entries, dependencies) in [
        (rewind.entries, rewind.dependency_entries),
        (forward.entries, forward.dependency_entries),
        (near.entries, near.dependency_entries),
    ] {
        assert_eq!(entries, goto.entries);
        assert_eq!(dependencies, goto.dependency_entries);
    }
}

#[tokio::test]
async fn empty_explicit_selection_is_invalid_instead_of_an_unfiltered_read() {
    let service = memory_service(EvidencedTemporalReader, EmptyNodeDetailReader);
    let request = temporal_move_request(None, ProtoDimensionSelection::default());
    let error = service
        .goto(Request::new(GotoRequest {
            about: request.about,
            cursor: Some(ProtoTemporalCursor {
                time: Some(ts(106)),
                ..Default::default()
            }),
            entry_selection: Some(TemporalEntrySelection { refs: vec![] }),
            ..Default::default()
        }))
        .await
        .expect_err("empty explicit selection");
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
}
