use kmp_domain::{AuthorNodeCard, NodeCard, NodeCardExpectation, NodeCardStatus};
use kmp_proto::v1beta1::{CondenseRequest, CondenseResponse, NodeCardView};
use tonic::Status;

use super::scalars::invalid_argument;

/// The only scope this increment admits.
///
/// A card declares exactly one dependency: the node body it names. Refusing
/// every other value is the checkable half of "no route-dependent truth
/// without declared dependencies" — the contract will not let a card declare
/// a path, a neighborhood or a clock, so it cannot stand on one.
pub const NODE_BODY_SCOPE: &str = "node_body";

/// Builds the command, with the kernel's own stamp for `authored_at`.
///
/// The instant is never taken from the caller: a backdated card would be
/// invisible to the cut a historical read stands at and would leak later text
/// into an earlier answer.
pub fn condense_command_from_proto(
    request: CondenseRequest,
    authored_at: String,
) -> Result<AuthorNodeCard, Box<Status>> {
    if request.scope != NODE_BODY_SCOPE {
        return Err(invalid_argument(format!(
            "scope must be `{NODE_BODY_SCOPE}`; this increment summarizes one stored body and \
             nothing a path or a clock would be needed for"
        )));
    }
    let expect = match (request.expect_absent, request.expect_card_revision) {
        (true, 0) => NodeCardExpectation::Absent,
        (false, revision) if revision > 0 => NodeCardExpectation::CardRevision(revision),
        _ => {
            return Err(invalid_argument(
                "declare exactly one expectation: expect_absent for the first card, or the \
                 card revision being replaced",
            ));
        }
    };
    for (field, value) in [
        ("about", &request.about),
        ("ref", &request.r#ref),
        ("language", &request.language),
        ("card", &request.card),
        ("source_record_digest", &request.source_record_digest),
        ("actor", &request.actor),
    ] {
        if value.trim().is_empty() {
            return Err(invalid_argument(format!("condense requires {field}")));
        }
    }
    if request.source_revision == 0 {
        return Err(invalid_argument(
            "condense requires the body revision the card was written from",
        ));
    }
    Ok(AuthorNodeCard {
        about: request.about,
        node_id: request.r#ref,
        language: request.language,
        text: request.card,
        source_revision: request.source_revision,
        source_content_hash: request.source_content_hash,
        source_record_digest: request.source_record_digest,
        expect,
        authored_by: request.actor,
        authored_at,
    })
}

pub fn condense_response_from_card(card: NodeCard) -> CondenseResponse {
    CondenseResponse {
        summary: format!(
            "Card revision {} stored for `{}` in {}, standing for body revision {}.",
            card.card_revision, card.node_id, card.language, card.source_revision
        ),
        r#ref: card.node_id.clone(),
        language: card.language.clone(),
        card: Some(NodeCardView {
            status: NodeCardStatus::Valid.as_str().into(),
            text_bytes: card.text.len() as u64,
            text: card.text,
            source_revision: card.source_revision,
            source_content_hash: card.source_content_hash,
            source_record_digest: card.source_record_digest,
            source_body_bytes: card.source_body_bytes,
            card_revision: card.card_revision,
            authored_by: card.authored_by,
            authored_at: card.authored_at,
        }),
        warnings: vec![
            "A card is a reader's derived view, never canonical memory and never proof. It \
             stands only for the body version it declares; a later write makes it stale, and a \
             stale card is never rendered as prose."
                .into(),
        ],
    }
}
