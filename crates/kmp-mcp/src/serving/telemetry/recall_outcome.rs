//! How a wake or an ask came out, as labels and counts: the status the
//! gate settled, why an UNKNOWN is one, the stated confidence, and how each
//! cited passage was reached — never the text of any of it.

use std::collections::BTreeMap;

use serde_json::Value;

use super::recorders::canonical_move;
use super::shape_reading::value_at;

/// Citations without a `reached_by` mark: the ones the question's own
/// terms matched.
const DIRECT: &str = "direct";
/// A label that is not a short snake_case word is not logged as itself.
const OTHER: &str = "other";
const LABEL_CHARS: usize = 40;

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct RecallOutcome {
    /// `answered`, `partial` or `unknown` when the anchored gate settled an
    /// ask; `None` for a wake and for an ask the gate did not settle.
    pub(crate) answer_status: Option<String>,
    pub(crate) unknown_reason: Option<String>,
    /// `proof.confidence` as stated to the caller.
    pub(crate) confidence: Option<String>,
    /// Ask only: whether the anchored gate decided (it states a status
    /// exactly when it did). `None` on a continuation page that does not
    /// repeat the first page's core, where the status is not restated.
    pub(crate) anchored: Option<bool>,
    /// `proof.evidence` on this page.
    pub(crate) citations: usize,
    /// Citations per `reached_by`, `name:count` sorted by name and joined
    /// by commas (`direct:2,semantic:1`); empty without citations.
    pub(crate) reached_by: String,
}

impl RecallOutcome {
    /// The outcome of a successful `kmp_ask` or `kmp_wake`; `None` for any
    /// other tool.
    pub(crate) fn from_tool_result(name: &str, result: &Value) -> Option<Self> {
        let is_ask = match canonical_move(name) {
            "kmp_ask" => true,
            "kmp_wake" => false,
            _ => return None,
        };
        let structured = result.get("structuredContent").unwrap_or(result);
        let answer_status = label_at(structured, &["answer_status"]);
        let core_reused = value_at(Some(structured), &["projection", "core_reused"])
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let evidence = value_at(Some(structured), &["proof", "evidence"])
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let mut reached: BTreeMap<String, usize> = BTreeMap::new();
        for item in evidence {
            let how = label_at(item, &["metadata", "reached_by"]).unwrap_or_else(|| DIRECT.into());
            *reached.entry(how).or_default() += 1;
        }
        Some(Self {
            anchored: (is_ask && !core_reused).then_some(answer_status.is_some()),
            answer_status,
            unknown_reason: label_at(structured, &["unknown_reason"]),
            confidence: label_at(structured, &["proof", "confidence"]),
            citations: evidence.len(),
            reached_by: reached
                .iter()
                .map(|(how, count)| format!("{how}:{count}"))
                .collect::<Vec<_>>()
                .join(","),
        })
    }
}

fn label_at(root: &Value, path: &[&str]) -> Option<String> {
    let text = value_at(Some(root), path)?.as_str()?;
    let word = !text.is_empty()
        && text.len() <= LABEL_CHARS
        && text
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    Some(if word { text } else { OTHER }.to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::RecallOutcome;

    #[test]
    fn an_anchored_ask_reports_status_confidence_and_how_citations_came() {
        let outcome = RecallOutcome::from_tool_result(
            "kmp_ask",
            &json!({"structuredContent": {
                "answer": "private answer text",
                "answer_status": "unknown",
                "unknown_reason": "attribute_not_found",
                "proof": {
                    "confidence": "low",
                    "evidence": [
                        {"id": "e1", "text": "private"},
                        {"id": "e2", "text": "private", "metadata": {"reached_by": "semantic"}},
                        {"id": "e3", "text": "private", "metadata": {"reached_by": "lifecycle"}},
                        {"id": "e4", "text": "private", "metadata": {"reached_by": "semantic"}},
                        {"id": "e5", "text": "private", "metadata": {"reached_by": "Not A Label!"}}
                    ]
                }
            }}),
        )
        .expect("ask");

        assert_eq!(outcome.answer_status.as_deref(), Some("unknown"));
        assert_eq!(
            outcome.unknown_reason.as_deref(),
            Some("attribute_not_found")
        );
        assert_eq!(outcome.confidence.as_deref(), Some("low"));
        assert_eq!(outcome.anchored, Some(true));
        assert_eq!(outcome.citations, 5);
        assert_eq!(
            outcome.reached_by,
            "direct:1,lifecycle:1,other:1,semantic:2"
        );
    }

    #[test]
    fn an_unanchored_ask_and_a_wake_state_what_they_have() {
        let ask = RecallOutcome::from_tool_result(
            "kmp_ask",
            &json!({"proof": {"confidence": "medium", "evidence": [{"id": "e"}]}}),
        )
        .expect("ask");
        assert_eq!(ask.anchored, Some(false));
        assert_eq!(ask.answer_status, None);
        assert_eq!(ask.reached_by, "direct:1");

        let page = RecallOutcome::from_tool_result(
            "kmp_ask",
            &json!({"projection": {"core_reused": true}, "proof": {"evidence": []}}),
        )
        .expect("page");
        assert_eq!(page.anchored, None);

        let wake = RecallOutcome::from_tool_result("kmp_wake", &json!({})).expect("wake");
        assert_eq!(wake.anchored, None);
        assert_eq!(wake.confidence, None);
        assert_eq!(wake.citations, 0);
        assert_eq!(wake.reached_by, "");

        assert_eq!(
            RecallOutcome::from_tool_result("kmp_inspect", &json!({})),
            None
        );
    }
}
