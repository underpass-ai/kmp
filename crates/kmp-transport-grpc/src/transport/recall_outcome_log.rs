//! How a Wake or an Ask came out, for the RPC's log line: the same labels
//! and counts the MCP server logs for `kmp_ask` and `kmp_wake`
//! (`kmp_mcp_tool`), read from the typed response. Never text.

use std::collections::BTreeMap;

use kmp_proto::v1beta1::{
    AnswerStatus, AskResponse, MemoryConfidence, MemoryEvidence, PageRequest, Proof,
    RecallProjection, UnknownReason, WakeResponse,
};

const REACHED_BY: &str = "reached_by";
const DIRECT: &str = "direct";
const OTHER: &str = "other";
const LABEL_CHARS: usize = 40;

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct RecallOutcomeLog {
    /// A page of an earlier call (its cursor), not a new call.
    pub(crate) is_continuation: bool,
    pub(crate) answer_status: Option<String>,
    pub(crate) unknown_reason: Option<String>,
    pub(crate) confidence: Option<String>,
    /// Ask only, and not on a continuation page that omits the core.
    pub(crate) anchored: Option<bool>,
    pub(crate) citations: usize,
    pub(crate) reached_by: String,
}

impl RecallOutcomeLog {
    pub(crate) fn from_ask(page: Option<&PageRequest>, response: &AskResponse) -> Self {
        let status = AnswerStatus::try_from(response.answer_status)
            .ok()
            .filter(|status| *status != AnswerStatus::Unspecified)
            .map(|status| label(status.as_str_name(), "ANSWER_STATUS_"));
        let reason = UnknownReason::try_from(response.unknown_reason)
            .ok()
            .filter(|reason| *reason != UnknownReason::Unspecified)
            .map(|reason| label(reason.as_str_name(), "UNKNOWN_REASON_"));
        let core_reused = core_reused(response.projection.as_ref());
        Self {
            anchored: (!core_reused).then_some(status.is_some()),
            answer_status: status,
            unknown_reason: reason,
            ..Self::from_proof(page, response.proof.as_ref())
        }
    }

    pub(crate) fn from_wake(page: Option<&PageRequest>, response: &WakeResponse) -> Self {
        Self::from_proof(page, response.proof.as_ref())
    }

    fn from_proof(page: Option<&PageRequest>, proof: Option<&Proof>) -> Self {
        let evidence = proof
            .map(|proof| proof.evidence.as_slice())
            .unwrap_or_default();
        Self {
            is_continuation: page.is_some_and(|page| !page.cursor.is_empty()),
            confidence: proof.map(|proof| {
                MemoryConfidence::try_from(proof.confidence)
                    .map(|confidence| label(confidence.as_str_name(), "MEMORY_CONFIDENCE_"))
                    .unwrap_or_else(|_| "unspecified".to_string())
            }),
            citations: evidence.len(),
            reached_by: reached_by(evidence),
            ..Self::default()
        }
    }
}

fn core_reused(projection: Option<&RecallProjection>) -> bool {
    projection.is_some_and(|projection| projection.core_reused)
}

fn label(name: &str, prefix: &str) -> String {
    name.strip_prefix(prefix)
        .unwrap_or(name)
        .to_ascii_lowercase()
}

fn reached_by(evidence: &[MemoryEvidence]) -> String {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for item in evidence {
        let how = match item.metadata.get(REACHED_BY).map(String::as_str) {
            None => DIRECT,
            Some(how) if is_label(how) => how,
            Some(_) => OTHER,
        };
        *counts.entry(how).or_default() += 1;
    }
    counts
        .iter()
        .map(|(how, count)| format!("{how}:{count}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn is_label(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= LABEL_CHARS
        && text
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

#[cfg(test)]
mod tests {
    use kmp_proto::v1beta1::{
        AnswerStatus, AskResponse, MemoryConfidence, MemoryEvidence, PageRequest, Proof,
        RecallProjection, UnknownReason, WakeResponse,
    };

    use super::RecallOutcomeLog;

    fn evidence(reached_by: Option<&str>) -> MemoryEvidence {
        let mut item = MemoryEvidence {
            text: "private".into(),
            ..MemoryEvidence::default()
        };
        if let Some(how) = reached_by {
            item.metadata.insert("reached_by".into(), how.into());
        }
        item
    }

    #[test]
    fn an_ask_logs_the_labels_the_mcp_line_logs() {
        let response = AskResponse {
            answer: "private".into(),
            answer_status: AnswerStatus::Unknown as i32,
            unknown_reason: UnknownReason::AttributeNotFound as i32,
            proof: Some(Proof {
                confidence: MemoryConfidence::Low as i32,
                evidence: vec![
                    evidence(None),
                    evidence(Some("semantic")),
                    evidence(Some("Odd Label")),
                ],
                ..Proof::default()
            }),
            ..AskResponse::default()
        };
        let log = RecallOutcomeLog::from_ask(None, &response);

        assert_eq!(
            log,
            RecallOutcomeLog {
                is_continuation: false,
                answer_status: Some("unknown".into()),
                unknown_reason: Some("attribute_not_found".into()),
                confidence: Some("low".into()),
                anchored: Some(true),
                citations: 3,
                reached_by: "direct:1,other:1,semantic:1".into(),
            }
        );
    }

    #[test]
    fn a_page_and_a_wake_say_what_they_carry() {
        let page = PageRequest {
            cursor: "next".into(),
            ..PageRequest::default()
        };
        let continued = AskResponse {
            projection: Some(RecallProjection {
                core_reused: true,
                ..RecallProjection::default()
            }),
            ..AskResponse::default()
        };
        let log = RecallOutcomeLog::from_ask(Some(&page), &continued);
        assert!(log.is_continuation);
        assert_eq!(log.anchored, None);
        assert_eq!(log.confidence, None);

        let unanchored = RecallOutcomeLog::from_ask(None, &AskResponse::default());
        assert_eq!(unanchored.anchored, Some(false));

        let wake = RecallOutcomeLog::from_wake(
            Some(&PageRequest::default()),
            &WakeResponse {
                proof: Some(Proof::default()),
                ..WakeResponse::default()
            },
        );
        assert!(!wake.is_continuation);
        assert_eq!(wake.anchored, None);
        assert_eq!(wake.confidence.as_deref(), Some("unspecified"));
        assert_eq!(wake.reached_by, "");
    }
}
