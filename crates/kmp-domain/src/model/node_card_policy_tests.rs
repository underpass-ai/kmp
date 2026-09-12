use std::collections::BTreeMap;

use super::*;

fn entry(about: &str) -> NodeProjection {
    NodeProjection {
        node_id: "about:claim:one".into(),
        node_kind: "memory_entry".into(),
        title: "One".into(),
        summary: "One".into(),
        status: "active".into(),
        labels: vec!["entry".into()],
        properties: BTreeMap::from([(MEMORY_ABOUT.to_string(), about.to_string())]),
        provenance: None,
    }
}

fn descriptor(revision: u64, digest: &str, body_bytes: u64) -> NodeBodyDescriptor {
    NodeBodyDescriptor {
        node_id: "about:claim:one".into(),
        revision,
        // Deliberately constant across revisions: the public token is not what
        // decides validity, and these tests would pass by accident if it were.
        content_hash: "sha256:public".into(),
        record_bytes: body_bytes + 64,
        body_bytes,
        record_digest: digest.into(),
    }
}

fn command() -> AuthorNodeCard {
    AuthorNodeCard {
        about: "question:cards".into(),
        node_id: "about:claim:one".into(),
        language: "es".into(),
        text: "Corto.".into(),
        source_revision: 3,
        source_content_hash: "sha256:public".into(),
        source_record_digest: "sha256:three".into(),
        expect: NodeCardExpectation::Absent,
        authored_by: "reader".into(),
        authored_at: "2026-09-12T09:00:00Z".into(),
    }
}

fn stored(card_revision: u64, digest: &str, authored_at: &str) -> NodeCard {
    NodeCard {
        node_id: "about:claim:one".into(),
        language: "es".into(),
        text: "Corto.".into(),
        source_revision: 3,
        source_content_hash: "sha256:public".into(),
        source_record_digest: digest.into(),
        source_body_bytes: 40,
        authored_by: "reader".into(),
        authored_at: authored_at.into(),
        card_revision,
    }
}

#[test]
fn a_first_card_against_the_stored_body_is_admitted_at_revision_one() {
    let node = entry("question:cards");
    let body = descriptor(3, "sha256:three", 40);

    let card = admit(&command(), Some(&node), Some(&body), None, None).expect("admitted");

    assert_eq!(card.card_revision, 1);
    assert_eq!(card.source_revision, 3);
    assert_eq!(card.source_record_digest, "sha256:three");
    assert_eq!(card.source_body_bytes, 40);
    assert_eq!(card.authored_at, "2026-09-12T09:00:00Z");
}

#[test]
fn a_body_that_moved_under_the_reader_is_refused_naming_both_versions() {
    let node = entry("question:cards");
    let moved = descriptor(4, "sha256:four", 40);

    let rejection = admit(&command(), Some(&node), Some(&moved), None, None).expect_err("refused");

    assert_eq!(
        rejection,
        NodeCardRejection::SourceMoved {
            declared_revision: 3,
            actual_revision: 4,
            declared_record_digest: "sha256:three".into(),
            actual_record_digest: "sha256:four".into(),
        }
    );
    assert!(rejection.is_conflict());
}

#[test]
fn rewritten_text_under_an_unchanged_public_token_is_still_refused() {
    // The failure the record digest exists for: the public content hash is
    // derived from the event and the node identity, so a write can leave it
    // and the revision alone while the stored text changes.
    let node = entry("question:cards");
    let rewritten = NodeBodyDescriptor {
        record_digest: "sha256:rewritten".into(),
        ..descriptor(3, "sha256:three", 40)
    };

    let rejection =
        admit(&command(), Some(&node), Some(&rewritten), None, None).expect_err("refused");

    assert!(matches!(
        rejection,
        NodeCardRejection::SourceMoved {
            declared_revision: 3,
            actual_revision: 3,
            ..
        }
    ));
}

#[test]
fn a_node_of_another_about_is_never_carded_from_this_one() {
    let foreign = entry("question:other");
    let body = descriptor(3, "sha256:three", 40);

    let rejection =
        admit(&command(), Some(&foreign), Some(&body), None, None).expect_err("refused");

    assert!(rejection.is_not_found(), "{rejection}");
}

#[test]
fn compare_and_set_refuses_both_directions() {
    let node = entry("question:cards");
    let body = descriptor(3, "sha256:three", 40);
    let existing = stored(2, "sha256:three", "2026-09-12T08:00:00Z");

    let declared_absent =
        admit(&command(), Some(&node), Some(&body), Some(&existing), None).expect_err("refused");
    assert_eq!(
        declared_absent,
        NodeCardRejection::CardAlreadyExists {
            actual_card_revision: 2
        }
    );

    let mut stale_expectation = command();
    stale_expectation.expect = NodeCardExpectation::CardRevision(1);
    let moved = admit(
        &stale_expectation,
        Some(&node),
        Some(&body),
        Some(&existing),
        None,
    )
    .expect_err("refused");
    assert_eq!(
        moved,
        NodeCardRejection::CardMoved {
            declared_card_revision: 1,
            actual_card_revision: 2
        }
    );

    let mut against_nothing = command();
    against_nothing.expect = NodeCardExpectation::CardRevision(1);
    let absent =
        admit(&against_nothing, Some(&node), Some(&body), None, None).expect_err("refused");
    assert_eq!(
        absent,
        NodeCardRejection::CardAbsent {
            declared_card_revision: 1
        }
    );

    let mut matching = command();
    matching.expect = NodeCardExpectation::CardRevision(2);
    let accepted =
        admit(&matching, Some(&node), Some(&body), Some(&existing), None).expect("admitted");
    assert_eq!(accepted.card_revision, 3);
}

#[test]
fn a_card_that_is_not_shorter_than_its_body_buys_nothing() {
    let node = entry("question:cards");
    let short_body = descriptor(3, "sha256:three", 4);

    let rejection =
        admit(&command(), Some(&node), Some(&short_body), None, None).expect_err("refused");

    assert!(matches!(rejection, NodeCardRejection::NotCompact { .. }));
    assert!(!rejection.is_conflict());
}

#[test]
fn a_card_record_is_itself_bounded() {
    let node = entry("question:cards");
    let huge_body = descriptor(3, "sha256:three", 1_000_000);
    let mut long = command();
    long.text = "x".repeat(MAX_CARD_BYTES + 1);

    let rejection = admit(&long, Some(&node), Some(&huge_body), None, None).expect_err("refused");

    assert!(matches!(rejection, NodeCardRejection::CardTooLarge { .. }));
}

#[test]
fn authorship_at_a_historical_cut_is_refused_rather_than_backdated() {
    let node = entry("question:cards");
    let body = descriptor(3, "sha256:three", 40);
    let cut = temporal_instant_nanos("2026-09-12T08:30:00Z").expect("cut");

    let rejection = admit(&command(), Some(&node), Some(&body), None, Some(cut))
        .expect_err("a card stamped after the cut cannot be authored under it");

    assert!(matches!(
        rejection,
        NodeCardRejection::AuthoredAfterCut { .. }
    ));
}

#[test]
fn presentation_shows_prose_only_for_the_body_version_the_card_declares() {
    let card = stored(1, "sha256:three", "2026-09-12T09:00:00Z");
    let current = descriptor(3, "sha256:three", 40);
    let moved = descriptor(4, "sha256:four", 40);

    let valid = presentation(Some(&card), Some(&current), None);
    assert_eq!(valid.status, NodeCardStatus::Valid);
    assert_eq!(valid.text.as_deref(), Some("Corto."));

    let stale = presentation(Some(&card), Some(&moved), None);
    assert_eq!(stale.status, NodeCardStatus::Stale);
    assert_eq!(stale.text, None, "stale prose is never returned");
    assert_eq!(
        stale.stored.expect("stamp").source_revision,
        3,
        "the reader still learns which body the card stands for"
    );
}

#[test]
fn a_card_over_a_node_with_no_stored_body_is_stale_not_valid() {
    let card = stored(1, "sha256:three", "2026-09-12T09:00:00Z");

    let presented = presentation(Some(&card), None, None);

    assert_eq!(presented.status, NodeCardStatus::Stale);
    assert_eq!(presented.text, None);
}

#[test]
fn a_historical_cut_checks_the_source_version_as_well_as_the_stamp() {
    let cut = temporal_instant_nanos("2026-09-12T08:30:00Z").expect("cut");
    let current = descriptor(3, "sha256:three", 40);
    let moved = descriptor(4, "sha256:four", 40);

    // Authored after the cut: named, never shown, even though its source
    // version matches the presented body.
    let later = stored(1, "sha256:three", "2026-09-12T09:00:00Z");
    let after = presentation(Some(&later), Some(&current), Some(cut));
    assert_eq!(after.status, NodeCardStatus::AfterCut);
    assert_eq!(after.text, None);

    // Authored before the cut but describing a body that has since moved:
    // the date gate alone would have let this through.
    let earlier = stored(1, "sha256:three", "2026-09-12T08:00:00Z");
    let stale = presentation(Some(&earlier), Some(&moved), Some(cut));
    assert_eq!(stale.status, NodeCardStatus::Stale);
    assert_eq!(stale.text, None);

    // Both gates pass.
    let shown = presentation(Some(&earlier), Some(&current), Some(cut));
    assert_eq!(shown.status, NodeCardStatus::Valid);
    assert_eq!(shown.text.as_deref(), Some("Corto."));
}

#[test]
fn an_unreadable_authorship_stamp_fails_the_cut_rather_than_passing_it() {
    let cut = temporal_instant_nanos("2026-09-12T08:30:00Z").expect("cut");
    let current = descriptor(3, "sha256:three", 40);
    let broken = stored(1, "sha256:three", "whenever");

    let presented = presentation(Some(&broken), Some(&current), Some(cut));

    assert_eq!(presented.status, NodeCardStatus::AfterCut);
    assert_eq!(presented.text, None);
}

#[test]
fn no_card_is_absent_and_carries_no_stamp() {
    let current = descriptor(3, "sha256:three", 40);

    let presented = presentation(None, Some(&current), None);

    assert_eq!(presented.status, NodeCardStatus::Absent);
    assert_eq!(presented.text, None);
    assert_eq!(presented.stored, None);
}
