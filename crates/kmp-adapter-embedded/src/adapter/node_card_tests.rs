use super::*;
use crate::EmbeddedKernelStore;
use kmp_domain::{
    NodeCardExpectation, NodeCardRejection, NodeDetailProjection, NodeDetailReader, NodeProjection,
    ProjectionMutation, ProjectionWriter, TraceSnapshotReader,
};

const ABOUT: &str = "question:cards";
const NODE: &str = "question:cards:claim:one";
const BODY: &str = "The reader read this whole body once, and it is long.";

fn node() -> ProjectionMutation {
    ProjectionMutation::UpsertNode(NodeProjection {
        node_id: NODE.into(),
        node_kind: "observation".into(),
        title: NODE.into(),
        summary: NODE.into(),
        status: "ACTIVE".into(),
        labels: vec!["entry".into()],
        properties: [("memory_about".into(), ABOUT.into())].into(),
        provenance: None,
    })
}

fn body(revision: u64, detail: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNodeDetail(NodeDetailProjection {
        node_id: NODE.into(),
        detail: detail.into(),
        content_hash: format!("sha256:{revision}"),
        revision,
    })
}

/// The digest the store holds for one node, taken from its descriptor — the
/// same value a reader receives from a trace before it writes a card.
async fn stored_digest(store: &EmbeddedKernelStore, node_id: &str) -> String {
    let tx = store.begin_read().expect("read");
    super::super::node_body_descriptor::read_one(tx.as_ref(), node_id)
        .expect("descriptor")
        .expect("the seeded body has a descriptor")
        .record_digest
}

fn command(revision: u64, digest: &str, expect: NodeCardExpectation) -> AuthorNodeCard {
    AuthorNodeCard {
        about: ABOUT.into(),
        node_id: NODE.into(),
        language: "es".into(),
        text: "El lector lo leyó.".into(),
        source_revision: revision,
        source_content_hash: format!("sha256:{revision}"),
        source_record_digest: digest.into(),
        expect,
        authored_by: "reader".into(),
        authored_at: "2026-09-12T09:00:00Z".into(),
    }
}

async fn seeded() -> (tempfile::TempDir, EmbeddedKernelStore, String) {
    let dir = tempfile::tempdir().expect("temporary store");
    let store = EmbeddedKernelStore::open(dir.path()).expect("open store");
    store
        .apply_mutations(vec![node(), body(1, BODY)])
        .await
        .expect("seed");
    let digest = stored_digest(&store, NODE).await;
    (dir, store, digest)
}

#[tokio::test]
async fn an_accepted_card_leaves_the_canonical_body_untouched() {
    let (_dir, store, digest) = seeded().await;
    let before = store
        .load_node_detail(NODE)
        .await
        .expect("read")
        .expect("body");

    store
        .author_node_card(command(1, &digest, NodeCardExpectation::Absent))
        .await
        .expect("store")
        .expect("admitted");

    let after = store
        .load_node_detail(NODE)
        .await
        .expect("read")
        .expect("body");
    assert_eq!(before, after, "a card never advances the canonical body");
}

#[tokio::test]
async fn a_refused_card_writes_nothing() {
    let (_dir, store, _digest) = seeded().await;

    let rejection = store
        .author_node_card(command(7, "sha256:never-stored", NodeCardExpectation::Absent))
        .await
        .expect("store")
        .expect_err("a body at revision 7 was never stored");

    assert!(matches!(
        rejection,
        NodeCardRejection::SourceMoved {
            declared_revision: 7,
            actual_revision: 1,
            ..
        }
    ));
    let tx = store.begin_read().expect("read");
    let cards = read_batch(tx.as_ref(), &[NODE.to_string()], "es").expect("card read");
    assert_eq!(cards, vec![None], "the refused write left no card behind");
}

#[tokio::test]
async fn compare_and_set_serializes_two_readers_on_one_node() {
    let (_dir, store, digest) = seeded().await;
    store
        .author_node_card(command(1, &digest, NodeCardExpectation::Absent))
        .await
        .expect("store")
        .expect("first card");

    let loser = store
        .author_node_card(command(1, &digest, NodeCardExpectation::Absent))
        .await
        .expect("store")
        .expect_err("the second reader declared a state that is no longer true");
    assert_eq!(
        loser,
        NodeCardRejection::CardAlreadyExists {
            actual_card_revision: 1
        }
    );

    let winner = store
        .author_node_card(command(1, &digest, NodeCardExpectation::CardRevision(1)))
        .await
        .expect("store")
        .expect("second card");
    assert_eq!(winner.card_revision, 2);
}

#[tokio::test]
async fn a_card_survives_reopening_the_store() {
    let dir = tempfile::tempdir().expect("temporary store");
    {
        let store = EmbeddedKernelStore::open(dir.path()).expect("open store");
        store
            .apply_mutations(vec![node(), body(1, BODY)])
            .await
            .expect("seed");
        let digest = stored_digest(&store, NODE).await;
        store
            .author_node_card(command(1, &digest, NodeCardExpectation::Absent))
            .await
            .expect("store")
            .expect("admitted");
    }

    let reopened = EmbeddedKernelStore::open(dir.path()).expect("reopen store");
    let tx = reopened.begin_read().expect("read");
    let cards = read_batch(tx.as_ref(), &[NODE.to_string()], "es").expect("card read");
    let card = cards[0].as_ref().expect("the card survived the restart");
    assert_eq!(card.card_revision, 1);
    assert_eq!(card.source_revision, 1);
    assert_eq!(card.authored_by, "reader");
}

#[tokio::test]
async fn language_is_part_of_the_identity_and_neither_card_shadows_the_other() {
    let (_dir, store, digest) = seeded().await;
    store
        .author_node_card(command(1, &digest, NodeCardExpectation::Absent))
        .await
        .expect("store")
        .expect("spanish card");
    let mut english = command(1, &digest, NodeCardExpectation::Absent);
    english.language = "en".into();
    english.text = "The reader read it.".into();
    store
        .author_node_card(english)
        .await
        .expect("store")
        .expect("english card");

    let tx = store.begin_read().expect("read");
    let ids = vec![NODE.to_string()];
    assert_eq!(
        read_batch(tx.as_ref(), &ids, "es").expect("read")[0]
            .as_ref()
            .expect("spanish")
            .text,
        "El lector lo leyó."
    );
    assert_eq!(
        read_batch(tx.as_ref(), &ids, "en").expect("read")[0]
            .as_ref()
            .expect("english")
            .text,
        "The reader read it."
    );
}

#[tokio::test]
async fn a_repeated_ref_reads_one_card_per_requested_slot() {
    let (_dir, store, digest) = seeded().await;
    store
        .author_node_card(command(1, &digest, NodeCardExpectation::Absent))
        .await
        .expect("store")
        .expect("admitted");

    let tx = store.begin_read().expect("read");
    let snapshot = super::super::trace_snapshot::TraceSnapshot(tx.as_ref());
    let cards = snapshot
        .cards(&[NODE.to_string(), "unknown".into(), NODE.to_string()], "es")
        .expect("card batch");

    assert_eq!(cards.len(), 3, "every requested slot is answered");
    assert_eq!(cards[0], cards[2]);
    assert_eq!(cards[1], None, "a node with no card is not an error");
}

const SOURCE: &str = "question:cards:evidence:one";
const SOURCE_BODY: &str = "The shared source body is the largest record on these paths, and every entry leans on it.";

fn source_node() -> ProjectionMutation {
    ProjectionMutation::UpsertNode(NodeProjection {
        node_id: SOURCE.into(),
        node_kind: "memory_evidence".into(),
        title: SOURCE.into(),
        summary: SOURCE.into(),
        status: "ACTIVE".into(),
        // No `entry` label: a source is admitted by its kind, and inventing
        // the label would be granting the permission rather than testing it.
        labels: vec![],
        properties: [("memory_about".into(), ABOUT.into())].into(),
        provenance: None,
    })
}

fn source_body() -> ProjectionMutation {
    ProjectionMutation::UpsertNodeDetail(NodeDetailProjection {
        node_id: SOURCE.into(),
        detail: SOURCE_BODY.into(),
        content_hash: "sha256:source-1".into(),
        revision: 1,
    })
}

#[tokio::test]
async fn a_shared_evidence_source_can_be_condensed_through_the_real_write_path() {
    let dir = tempfile::tempdir().expect("temporary store");
    let store = EmbeddedKernelStore::open(dir.path()).expect("open store");
    store
        .apply_mutations(vec![node(), body(1, BODY), source_node(), source_body()])
        .await
        .expect("seed");
    let digest = stored_digest(&store, SOURCE).await;

    let mut command = command(1, &digest, NodeCardExpectation::Absent);
    command.node_id = SOURCE.into();
    command.text = "La fuente compartida sostiene ambas entradas.".into();
    let card = store
        .author_node_card(command)
        .await
        .expect("store")
        .expect("an evidence source of this about is condensable");

    assert_eq!(card.node_id, SOURCE);
    assert_eq!(card.card_revision, 1);
    assert_eq!(card.source_record_digest, digest);
    assert_eq!(card.source_body_bytes, SOURCE_BODY.len() as u64);

    // And it reads back as valid against the descriptor, which is what a
    // compact trace consults — never the record.
    let tx = store.begin_read().expect("read");
    let stored = read_batch(tx.as_ref(), &[SOURCE.to_string()], "es")
        .expect("card read")
        .remove(0)
        .expect("stored card");
    let descriptor = super::super::node_body_descriptor::read_one(tx.as_ref(), SOURCE)
        .expect("descriptor")
        .expect("descriptor");
    assert!(kmp_domain::node_card_policy::describes(&stored, &descriptor));
}

#[tokio::test]
async fn a_ref_of_another_about_is_refused_however_it_is_labelled() {
    let dir = tempfile::tempdir().expect("temporary store");
    let store = EmbeddedKernelStore::open(dir.path()).expect("open store");
    let mut foreign = source_node();
    if let ProjectionMutation::UpsertNode(node) = &mut foreign {
        node.properties
            .insert("memory_about".into(), "question:other".into());
    }
    store
        .apply_mutations(vec![foreign, source_body()])
        .await
        .expect("seed");
    let digest = stored_digest(&store, SOURCE).await;

    let mut command = command(1, &digest, NodeCardExpectation::Absent);
    command.node_id = SOURCE.into();
    command.text = "No debería escribirse.".into();
    let rejection = store
        .author_node_card(command)
        .await
        .expect("store")
        .expect_err("a source of another about is not condensable from here");

    assert!(rejection.is_not_found(), "{rejection}");
}


#[tokio::test]
async fn a_body_rewritten_under_the_same_public_tokens_is_refused_not_delivered() {
    // The case the record digest exists for: same revision, same public
    // content hash, same record length, different bytes. Every comparison a
    // reader can make without the record agrees; only the digest disagrees.
    let dir = tempfile::tempdir().expect("temporary store");
    let store = EmbeddedKernelStore::open(dir.path()).expect("open store");
    store
        .apply_mutations(vec![node(), body(1, BODY)])
        .await
        .expect("seed");
    let before = stored_digest(&store, NODE).await;

    let rewritten: String = BODY
        .chars()
        .map(|c| if c == 'r' { 'R' } else { c })
        .collect();
    assert_eq!(rewritten.len(), BODY.len(), "same length, different bytes");
    assert_ne!(rewritten, BODY);

    // A write that leaves the public tokens alone. The header moves with the
    // record, so this is the store staying consistent with itself.
    store
        .apply_mutations(vec![ProjectionMutation::UpsertNodeDetail(
            NodeDetailProjection {
                node_id: NODE.into(),
                detail: rewritten,
                content_hash: "sha256:1".into(),
                revision: 1,
            },
        )])
        .await
        .expect("rewrite");

    let after = stored_digest(&store, NODE).await;
    assert_ne!(
        before, after,
        "the digest moves with the bytes even when revision and hash do not"
    );

    // A card bound to the old digest no longer describes this body, so a
    // compact read reports it stale rather than showing its prose.
    let tx = store.begin_read().expect("read");
    let descriptor = super::super::node_body_descriptor::read_one(tx.as_ref(), NODE)
        .expect("descriptor")
        .expect("descriptor");
    let stale = NodeCard {
        node_id: NODE.into(),
        language: "es".into(),
        text: "Resumen viejo.".into(),
        source_revision: 1,
        source_content_hash: "sha256:1".into(),
        source_record_digest: before,
        source_body_bytes: BODY.len() as u64,
        authored_by: "reader".into(),
        authored_at: "2026-09-12T09:00:00Z".into(),
        card_revision: 1,
    };
    assert!(
        !kmp_domain::node_card_policy::describes(&stale, &descriptor),
        "an unchanged revision and content hash do not make a card current"
    );
}

#[tokio::test]
async fn a_body_whose_record_disagrees_with_its_descriptor_is_never_delivered() {
    use kmp_domain::TraceSnapshotReader;

    let dir = tempfile::tempdir().expect("temporary store");
    let store = EmbeddedKernelStore::open(dir.path()).expect("open store");
    store
        .apply_mutations(vec![node(), body(1, BODY)])
        .await
        .expect("seed");

    // Only the header's digest is changed, and only to another well-formed
    // one. Its record length still matches the stored record, so nothing but
    // the digest itself can catch this.
    {
        let tx = store.begin_read().expect("read");
        let raw = tx
            .get(Table::DetailHeaders, Key::Str(NODE))
            .expect("read")
            .expect("header");
        let mut header: serde_json::Value = serde_json::from_slice(&raw).expect("header json");
        header["record_digest"] = serde_json::json!(format!("sha256:{}", "0".repeat(64)));
        drop(tx);
        let mut write = store.begin_write().expect("write");
        write
            .insert(
                Table::DetailHeaders,
                Key::Str(NODE),
                &serde_json::to_vec(&header).expect("header bytes"),
            )
            .expect("forge header");
        write.commit().expect("commit");
    }

    let tx = store.begin_read().expect("read");
    let snapshot = super::super::trace_snapshot::TraceSnapshot(tx.as_ref());
    let error = snapshot
        .verified_bodies(&[NODE.to_string()])
        .expect_err("a body that does not match its descriptor is not returned");

    assert!(
        error.to_string().contains("does not match the digest"),
        "{error}"
    );
}
