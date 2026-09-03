//! Which memories still owe the reader an English search summary.
//!
//! A memory written before summaries existed, or written in a language the
//! kernel cannot reach from English, is found from an English question only
//! through a `summary_en` its writer attached. The kernel cannot write one —
//! that is the model's job, at the one moment the kernel admits a model — but
//! it can say exactly which memories need one and which carry one that will
//! not carry retrieval. This reads that list off the store's own event log,
//! the way `document` reads an about: the latest write of every entry wins.
//!
//! One rule, the writer's: an entry whose text does not lean to English and
//! has no valid summary is pending; an entry whose summary fails the lint is
//! pending whatever its language, and the faults travel with it; an English
//! entry with no summary is not pending — an English question already
//! reaches it.

mod pending_summary;

use std::collections::HashMap;

use kmp_domain::SearchSummary;
use kmp_domain::language::{KERNEL_LANGUAGE, LanguageVocabulary};
use serde_json::Value;

pub use pending_summary::PendingSummary;

/// The memories in an exported bundle that owe a summary, in the order the
/// store met them. `about` narrows to one about; `None` reads them all.
pub fn pending(bundle: &str, about: Option<&str>) -> Result<Vec<PendingSummary>, String> {
    let mut latest: Vec<(String, String, Value)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for (number, line) in bundle.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let event: Value = serde_json::from_str(line)
            .map_err(|error| format!("bundle line {} is not JSON: {error}", number + 1))?;
        let Some(root) = event.get("root_node_id").and_then(Value::as_str) else {
            continue;
        };
        if about.is_some_and(|about| about != root) {
            continue;
        }
        for change in event["changes"].as_array().into_iter().flatten() {
            if change["entity_kind"].as_str() != Some("memory_entry") {
                continue;
            }
            let Some(id) = change["entity_id"].as_str() else {
                continue;
            };
            let payload: Value = change["payload_json"]
                .as_str()
                .and_then(|raw| serde_json::from_str(raw).ok())
                .unwrap_or(Value::Null);
            match index.get(id) {
                Some(&at) => latest[at] = (root.to_string(), id.to_string(), payload),
                None => {
                    index.insert(id.to_string(), latest.len());
                    latest.push((root.to_string(), id.to_string(), payload));
                }
            }
        }
    }

    Ok(latest
        .into_iter()
        .filter_map(|(about, reference, payload)| judge(about, reference, &payload))
        .collect())
}

/// Whether one entry owes a summary, and why.
fn judge(about: String, reference: String, payload: &Value) -> Option<PendingSummary> {
    let text = payload["text"].as_str().unwrap_or_default().to_string();
    let kind = payload["kind"].as_str().unwrap_or("entry").to_string();
    let summary = payload["metadata"][SearchSummary::METADATA_KEY].as_str();
    match summary {
        Some(summary) => match SearchSummary::lint(&text, summary) {
            Ok(_) => None,
            Err(faults) => Some(PendingSummary {
                about,
                reference,
                kind,
                text,
                faults: faults.iter().map(ToString::to_string).collect(),
            }),
        },
        None => {
            let leans = LanguageVocabulary::shipped().leans_in(&text);
            if leans.is_none_or(|language| language == KERNEL_LANGUAGE) {
                return None;
            }
            Some(PendingSummary {
                about,
                reference,
                kind,
                text,
                faults: Vec::new(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(about: &str, id: &str, payload: &str) -> String {
        serde_json::json!({
            "root_node_id": about,
            "changes": [{
                "entity_kind": "memory_entry",
                "entity_id": id,
                "payload_json": payload
            }]
        })
        .to_string()
    }

    fn payload(text: &str, summary: Option<&str>) -> String {
        let mut value = serde_json::json!({"id": "x", "kind": "decision", "text": text});
        if let Some(summary) = summary {
            value["metadata"] = serde_json::json!({"summary_en": summary});
        }
        value.to_string()
    }

    #[test]
    fn a_spanish_memory_without_a_summary_is_pending_and_an_english_one_is_not() {
        let bundle = [
            event(
                "project:a",
                "project:a:e1",
                &payload(
                    "El despliegue se retrasó porque los auditores no firmaron.",
                    None,
                ),
            ),
            event(
                "project:a",
                "project:a:e2",
                &payload(
                    "The rollout slipped because the auditors had not signed off.",
                    None,
                ),
            ),
        ]
        .join("\n");

        let pending = pending(&bundle, None).expect("bundle parses");

        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].reference, "project:a:e1");
        assert!(pending[0].faults.is_empty());
    }

    #[test]
    fn a_summary_that_fails_the_lint_is_pending_with_its_faults_whatever_the_language() {
        let bundle = event(
            "project:a",
            "project:a:e1",
            &payload(
                "The v0.7.0 rollout slipped because the auditors had not signed off.",
                Some("The launch was postponed because the audit sign-off was missing."),
            ),
        );

        let pending = pending(&bundle, None).expect("bundle parses");

        assert_eq!(pending.len(), 1);
        assert_eq!(
            pending[0].faults,
            ["drops identifiers the text carries: v0.7.0"]
        );
    }

    #[test]
    fn the_latest_write_of_an_entry_decides_and_an_about_filter_narrows() {
        let bundle = [
            event(
                "project:a",
                "project:a:e1",
                &payload("La válvula se congeló durante el turno de noche.", None),
            ),
            event(
                "project:a",
                "project:a:e1",
                &payload(
                    "La válvula se congeló durante el turno de noche.",
                    Some("The valve froze during the night shift."),
                ),
            ),
            event(
                "project:b",
                "project:b:e1",
                &payload("El menú del comedor se publicó en el tablón.", None),
            ),
        ]
        .join("\n");

        assert!(
            pending(&bundle, Some("project:a"))
                .expect("parses")
                .is_empty()
        );
        let all = pending(&bundle, None).expect("parses");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].about, "project:b");
    }

    #[test]
    fn a_line_that_is_not_json_is_named() {
        let error = pending("{\"root_node_id\":\"a\"}\nnot json", None).expect_err("bad line");

        assert!(error.starts_with("bundle line 2 is not JSON"), "{error}");
    }
}
