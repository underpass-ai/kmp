//! Which proposed search expansions a write keeps (P15, Doc2Query--).
//!
//! A writer proposes, per memory, a few short ways a reader may ask for it.
//! Each is read first by the deterministic lint (`SearchExpansions::lint`)
//! and then by Jev against the stored text; only those Jev reads as
//! belonging at the store's bar are kept. No judge, no expansion: nothing is
//! stored without a judgement.

use std::collections::BTreeMap;

use kmp_domain::{SearchExpansionFault, SearchExpansions, SearchSummary};
use serde_json::{Map, Value, json};

use super::json_value_type::JsonValueType;
use super::validation_error::WriteValidationError;

/// One memory's proposal, read against its stored text.
struct Proposal {
    reference: String,
    text: String,
    /// What passed the lint, in the writer's order, awaiting the judge.
    linted: Vec<String>,
}

/// The expansions of one write, from proposal to what is stored.
#[derive(Default)]
pub(crate) struct ExpansionSelection {
    proposals: Vec<Proposal>,
    refused: Vec<Value>,
    stored: BTreeMap<String, Vec<String>>,
    judged_by: Option<String>,
    jev: Option<Value>,
    not_judged: Option<String>,
}

impl ExpansionSelection {
    /// The shape of one memory's `search_expansions`: at most
    /// [`SearchExpansions::MAX_EXPANSIONS`] strings. Their content is read
    /// later, against the stored text.
    pub(crate) fn proposed(
        value: Option<&Value>,
        field: &str,
    ) -> Result<Vec<String>, WriteValidationError> {
        let Some(value) = value else {
            return Ok(Vec::new());
        };
        let items = value
            .as_array()
            .ok_or_else(|| WriteValidationError::wrong_type(field, JsonValueType::Array, value))?;
        if items.len() > SearchExpansions::MAX_EXPANSIONS {
            return Err(WriteValidationError::new(format!(
                "{field} holds {} expansions; at most {} are kept per memory",
                items.len(),
                SearchExpansions::MAX_EXPANSIONS
            ))
            .at(field)
            .code("TOO_MANY_EXPANSIONS"));
        }
        items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                item.as_str().map(str::to_string).ok_or_else(|| {
                    WriteValidationError::wrong_type(
                        &format!("{field}[{index}]"),
                        JsonValueType::String,
                        item,
                    )
                })
            })
            .collect()
    }

    /// Reads every proposal with the lint; what fails it is refused with its
    /// faults and never reaches the judge.
    pub(crate) fn lint(proposals: impl IntoIterator<Item = (String, String, Vec<String>)>) -> Self {
        let mut selection = Self::default();
        for (reference, text, proposed) in proposals {
            let mut linted = Vec::new();
            for expansion in proposed {
                match SearchExpansions::lint(&text, &expansion, &linted) {
                    Ok(kept) => linted.push(kept),
                    Err(faults) => selection.refused.push(json!({
                        "ref": reference,
                        "expansion": expansion,
                        "why": SearchExpansionFault::describe(&faults),
                    })),
                }
            }
            selection.proposals.push(Proposal {
                reference,
                text,
                linted,
            });
        }
        selection
    }

    /// Whether any proposal was made at all.
    pub(crate) fn is_empty(&self) -> bool {
        self.proposals.is_empty()
    }

    /// Whether an expansion passed the lint and awaits the judge.
    pub(crate) fn needs_judgement(&self) -> bool {
        self.proposals
            .iter()
            .any(|proposal| !proposal.linted.is_empty())
    }

    /// The internal `kmp_curate` call that judges what passed the lint.
    pub(crate) fn judge_arguments(&self, about: &str) -> Value {
        json!({
            "mode": "judge_expansions",
            "about": about,
            "items": self.proposals.iter()
                .filter(|proposal| !proposal.linted.is_empty())
                .map(|proposal| json!({
                    "ref": proposal.reference,
                    "text": proposal.text,
                    "expansions": proposal.linted,
                }))
                .collect::<Vec<_>>(),
        })
    }

    /// Keeps what the judge accepted at its bar. An answer that is not an
    /// enabled judgement keeps nothing and says why.
    pub(crate) fn apply(&mut self, answer: &Value) {
        if answer.get("enabled") != Some(&Value::Bool(true)) {
            self.not_judged = Some(
                answer
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("Jev did not judge the expansions")
                    .to_string(),
            );
            return;
        }
        let accept_at = answer
            .get("accept_at")
            .and_then(Value::as_f64)
            .unwrap_or(1.0);
        self.judged_by = answer
            .get("judged_by")
            .and_then(Value::as_str)
            .map(str::to_string);
        self.jev = answer.get("jev").cloned();
        let verdicts = answer
            .get("verdicts")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for proposal in self
            .proposals
            .iter()
            .filter(|proposal| !proposal.linted.is_empty())
        {
            let yes = verdicts
                .iter()
                .find(|verdict| verdict["ref"].as_str() == Some(proposal.reference.as_str()))
                .and_then(|verdict| verdict["yes"].as_array())
                .cloned()
                .unwrap_or_default();
            let mut kept = Vec::new();
            for (index, expansion) in proposal.linted.iter().enumerate() {
                match yes.get(index).and_then(Value::as_f64) {
                    Some(probability) if probability >= accept_at => kept.push(expansion.clone()),
                    probability => self.refused.push(json!({
                        "ref": proposal.reference,
                        "expansion": expansion,
                        "why": match probability {
                            Some(probability) => format!(
                                "Jev reads it as not belonging to the memory ({probability:.2} < {accept_at:.2})"
                            ),
                            None => "Jev did not answer for it".to_string(),
                        },
                    })),
                }
            }
            if !kept.is_empty() {
                self.stored.insert(proposal.reference.clone(), kept);
            }
        }
    }

    /// Records that nothing could be judged, and why.
    pub(crate) fn not_judged(&mut self, reason: impl Into<String>) {
        self.not_judged = Some(reason.into());
    }

    /// The metadata that stores a memory's kept expansions, bound to the
    /// text they were judged against; `None` when none was kept.
    pub(crate) fn metadata_for(&self, reference: &str) -> Option<Map<String, Value>> {
        let kept = self.stored.get(reference)?;
        let proposal = self
            .proposals
            .iter()
            .find(|proposal| proposal.reference == reference)?;
        let mut metadata = Map::new();
        metadata.insert(
            SearchExpansions::METADATA_KEY.to_string(),
            json!(SearchExpansions::render(kept)),
        );
        metadata.insert(
            SearchExpansions::SOURCE_FINGERPRINT_METADATA_KEY.to_string(),
            json!(SearchSummary::source_fingerprint(&proposal.text)),
        );
        metadata.insert(
            SearchExpansions::JUDGED_BY_METADATA_KEY.to_string(),
            json!(self.judged_by.clone().unwrap_or_default()),
        );
        Some(metadata)
    }

    /// Whether anything is to be stored.
    pub(crate) fn stores_any(&self) -> bool {
        !self.stored.is_empty()
    }

    /// What the write says about its expansions.
    pub(crate) fn report(&self) -> Value {
        let mut report = json!({
            "stored": self.stored,
            "refused": self.refused,
        });
        if let Some(judged_by) = &self.judged_by {
            report["judged_by"] = json!(judged_by);
        }
        if let Some(jev) = &self.jev {
            report["jev"] = jev.clone();
        }
        if let Some(reason) = &self.not_judged {
            report["not_stored"] = json!(reason);
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "The rollout slipped because the auditors had not signed off.";

    fn selection() -> ExpansionSelection {
        ExpansionSelection::lint([(
            "p:a:e1".to_string(),
            TEXT.to_string(),
            vec![
                "Why was the launch postponed?".to_string(),
                "the rollout slipped".to_string(),
                "launch delay audit".to_string(),
            ],
        )])
    }

    #[test]
    fn the_shape_is_checked_before_anything_is_read() {
        assert_eq!(ExpansionSelection::proposed(None, "x"), Ok(Vec::new()));
        assert_eq!(
            ExpansionSelection::proposed(Some(&json!(["a b"])), "x"),
            Ok(vec!["a b".to_string()])
        );
        assert!(ExpansionSelection::proposed(Some(&json!("a b")), "x").is_err());
        assert!(ExpansionSelection::proposed(Some(&json!([1])), "x").is_err());
        let seven = json!(["a", "b", "c", "d", "e", "f", "g"]);
        let error = ExpansionSelection::proposed(Some(&seven), "x").expect_err("too many");
        assert!(error.message.contains("at most 6"), "{error:?}");
    }

    #[test]
    fn the_lint_refuses_before_the_judge_and_the_judge_decides_the_rest() {
        let mut selection = selection();
        assert!(selection.needs_judgement());
        let arguments = selection.judge_arguments("p:a");
        assert_eq!(arguments["mode"], "judge_expansions");
        assert_eq!(
            arguments["items"][0]["expansions"],
            json!(["Why was the launch postponed?", "launch delay audit"])
        );
        selection.apply(&json!({
            "enabled": true, "accept_at": 0.7, "judged_by": "jev noul>=0.70",
            "verdicts": [{"ref": "p:a:e1", "yes": [0.9, 0.2]}],
            "jev": {"requests": 1, "input_tokens": 90}
        }));
        let report = selection.report();
        assert_eq!(
            report["stored"]["p:a:e1"],
            json!(["Why was the launch postponed?"])
        );
        assert_eq!(report["refused"].as_array().expect("refused").len(), 2);
        assert!(
            report["refused"][0]["why"]
                .as_str()
                .expect("why")
                .contains("repeats")
        );
        assert!(
            report["refused"][1]["why"]
                .as_str()
                .expect("why")
                .contains("0.20 < 0.70")
        );
        assert_eq!(report["jev"]["input_tokens"], 90);
        let metadata = selection.metadata_for("p:a:e1").expect("kept");
        assert_eq!(
            metadata[SearchExpansions::METADATA_KEY],
            "Why was the launch postponed?"
        );
        assert_eq!(
            metadata[SearchExpansions::SOURCE_FINGERPRINT_METADATA_KEY],
            SearchSummary::source_fingerprint(TEXT)
        );
        assert_eq!(
            metadata[SearchExpansions::JUDGED_BY_METADATA_KEY],
            "jev noul>=0.70"
        );
        assert!(selection.stores_any());
    }

    #[test]
    fn without_a_judgement_nothing_is_kept() {
        let mut selection = selection();
        selection.apply(&json!({"enabled": false, "reason": "the store has not opted in"}));
        assert!(!selection.stores_any());
        assert!(selection.metadata_for("p:a:e1").is_none());
        assert_eq!(
            selection.report()["not_stored"],
            "the store has not opted in"
        );

        let mut selection = selection_with_unanswered();
        selection.not_judged("backend without Jev");
        assert!(!selection.stores_any());
    }

    fn selection_with_unanswered() -> ExpansionSelection {
        let mut selection = selection();
        selection.apply(&json!({"enabled": true, "accept_at": 0.5, "verdicts": []}));
        assert!(
            !selection.stores_any(),
            "an unanswered expansion is refused"
        );
        selection
    }
}
