use crate::guidance::GuidancePurpose;
use serde_json::{Value, json};

/// Recommendations interpret typed response fields, never diagnostic prose.
/// They propose one move; they neither execute it nor certify the writer's claims.
pub(super) struct GuidanceRecommendation;

impl GuidanceRecommendation {
    pub(super) fn choose(
        purpose: GuidancePurpose,
        tool: &str,
        arguments: &Value,
        result: &Value,
    ) -> Option<Value> {
        let body = &result["structuredContent"];
        if result["isError"] == true {
            if let Some((index, action)) = body["feedback"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
                .find_map(|(i, item)| valid_call(&item["action"]).map(|call| (i, call)))
            {
                return Some(proposal(
                    "repair_or_restart",
                    "The producer returned a repair or restart. Review it before retrying.",
                    &format!("/feedback/{index}/action"),
                    action,
                    false,
                ));
            }
            // A validation refusal is the caller's to repair: its feedback
            // names each field. Only failures no argument can fix stop.
            if body["error"]["code"] == "invalid_argument" {
                return Some(repair_refused_arguments(body));
            }
            return Some(
                json!({"reason_code":"operation_refused","reason":"Keep the original error and feedback; do not infer a repair from wording.","basis":{"error_code":body["error"]["code"]},"stop":true}),
            );
        }
        if tool == "kmp_write_memory" && body["status"] == "needs_review" {
            return Some(json!({"reason_code":"review_write_neighborhood",
                "reason":"Nothing was written. Review the neighborhood, proposed directions and omissions; expand if needed. Resume the returned write only after reviewing, or correct the proposal.",
                "basis":{"context_pointer":"/neighborhood","resume_pointer":"/next_actions/0"}, "review_required":true, "stop":true}));
        }
        let partial = super::tool_result::packet_is_partial(body);
        let action = valid_call(&body["projection"]["next_action"])
            .map(|a| ("/projection/next_action".to_owned(), a))
            .or_else(|| {
                body["next_actions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                    .find_map(|(i, a)| valid_call(a).map(|a| (format!("/next_actions/{i}"), a)))
            });
        if partial {
            return Some(match action {
                Some((pointer, action)) => proposal(
                    "finish_selected_packet",
                    "Finish the selected packet before exploring or treating missing proof as absence.",
                    &pointer,
                    action,
                    false,
                ),
                None => {
                    json!({"reason_code":"partial_without_action","reason":"The packet is partial but carries no executable continuation. Do not invent a cursor or claim complete proof.","stop":true})
                }
            });
        }
        // UNKNOWN is the protocol's exact answer value, not text classification.
        if tool == "kmp_ask" && body["answer"] == "UNKNOWN" {
            return Some(
                json!({"reason_code":"unknown_in_selection","reason":"UNKNOWN is a valid stopping result in this selection. An outside match does not answer inside it.","basis":{"missing":body["proof"]["missing"],"nearest_outside":body["proof"]["nearest_outside"],"scope_pointer":"/proof"},"stop":true}),
            );
        }
        if purpose == GuidancePurpose::History {
            if let Some((pointer, action)) = action {
                return Some(proposal(
                    "native_history_move",
                    "The kernel supplied this next historical position with its clock and dimensions. This starts a new selection.",
                    &pointer,
                    action,
                    true,
                ));
            }
            if tool == "kmp_inspect" {
                return Some(proposal(
                    "history_needs_clock",
                    "An inspection is current. Choose the clock and temporal selection before using it as historical proof.",
                    "/object",
                    json!({"tool":"kmp_guide","arguments":{"topic":"time"}}),
                    true,
                ));
            }
        }
        if purpose == GuidancePurpose::Audit && tool == "kmp_inspect" {
            let mut links = Vec::new();
            for direction in ["incoming", "outgoing"] {
                for (index, link) in body["links"][direction]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    if link["class"] == "structural"
                        || matches!(
                            link["rel"].as_str(),
                            Some("supports" | "contains_entry" | "has_dimension")
                        )
                    {
                        continue;
                    }
                    if let (Some(from), Some(to), Some(rel)) = (
                        link["from"].as_str(),
                        link["to"].as_str(),
                        link["rel"].as_str(),
                    ) {
                        if from == to {
                            continue;
                        }
                        links.push((rel, from, to, format!("/links/{direction}/{index}")));
                    }
                }
            }
            // Stable order, not a learned importance score. All remain in links.
            links.sort();
            if let Some((rel, from, to, pointer)) = links.first()
                && let Some(about) = arguments["about"].as_str()
            {
                let mut advice = proposal(
                    "audit_declared_relation",
                    "Trace this declared relation and assess its stored why/evidence. Its presence is not independent proof of truth.",
                    pointer,
                    json!({"tool":"kmp_trace","arguments":{"about":about,"from":from,"to":to}}),
                    true,
                );
                advice["basis"]["relation"] = json!({"from":from,"to":to,"rel":rel});
                advice["alternatives_in_response"] = json!(links.len() - 1);
                return Some(advice);
            }
        }
        if matches!(purpose, GuidancePurpose::Audit | GuidancePurpose::Answer)
            && let Some(about) = arguments["about"].as_str()
        {
            // Claim candidates carry their canonical ref in supports. Their
            // source can be an external citation; never use it as a node id or
            // manufacture a ref by removing an evidence wrapper prefix.
            // An about-wide packet proves ownership only when it read one
            // about. A multi-about source needs its own explicit ownership.
            let selected = body["proof"]["abouts_selected"].as_array();
            if !selected.is_some_and(|abouts| abouts.len() == 1 && abouts[0] == about) {
                return None;
            }
            if let Some((index, node)) = body["proof"]["evidence"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
                .find_map(|(i, item)| {
                    matches!(
                        item["metadata"]["proof_role"].as_str(),
                        Some("entry_text" | "entry_detail")
                    )
                    .then(|| item["supports"].as_array())
                    .flatten()
                    .filter(|refs| refs.len() == 1)
                    .and_then(|refs| refs[0].as_str())
                    .map(|reference| (i, reference))
                })
            {
                return Some(proposal(
                    "inspect_retrieved_claim",
                    "Inspect the current claim before relying on it. This is a fresh read without the earlier temporal or dimensional filters, not historical proof.",
                    &format!("/proof/evidence/{index}"),
                    json!({"tool":"kmp_inspect","arguments":{"about":about,"ref":node}}),
                    true,
                ));
            }
        }
        None
    }
}

/// Repair, not stop: the refused fields are typed in `feedback`, or, without
/// feedback, named by the error of a call that cannot succeed unchanged.
fn repair_refused_arguments(body: &Value) -> Value {
    let listed = body["feedback"]
        .as_array()
        .is_some_and(|feedback| !feedback.is_empty());
    if listed {
        json!({"reason_code":"repair_listed_fields",
            "reason":"Repair every field listed in feedback and retry the whole call. Fix each from the source; never remove a required field or invent evidence to pass.",
            "basis":{"error_code":body["error"]["code"],"feedback_pointer":"/feedback"}})
    } else {
        json!({"reason_code":"repair_arguments",
            "reason":"Repair the arguments the error names and retry; the same call unchanged cannot succeed.",
            "basis":{"error_code":body["error"]["code"]}})
    }
}

fn valid_call(value: &Value) -> Option<Value> {
    (value["tool"]
        .as_str()
        .is_some_and(|tool| super::tool_error_help::ToolErrorHelp::topic(tool).is_some())
        && value["arguments"].is_object())
    .then(|| value.clone())
}

fn proposal(
    code: &str,
    reason: &str,
    pointer: &str,
    action: Value,
    changes_selection: bool,
) -> Value {
    json!({"reason_code":code,"reason":reason,"basis":{"response_pointer":pointer},"action":action,"changes_selection":changes_selection})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn advise(tool: &str, result: Value) -> Value {
        GuidanceRecommendation::choose(GuidancePurpose::Continue, tool, &json!({}), &result)
            .expect("a recommendation")
    }

    #[test]
    fn a_pending_review_stops_and_still_asks_for_the_review() {
        let advice = advise(
            "kmp_write_memory",
            json!({"isError": false, "structuredContent": {"status": "needs_review"}}),
        );
        assert_eq!(advice["reason_code"], "review_write_neighborhood");
        assert_eq!(advice["review_required"], true);
        assert_eq!(advice["stop"], true);
    }

    #[test]
    fn validation_refusals_ask_for_repair_and_other_errors_stop() {
        let listed = advise(
            "kmp_write_memory",
            json!({"isError": true, "structuredContent": {"error": {"code": "invalid_argument"},
                "feedback": [{"field": "memories[0].labels"}]}}),
        );
        assert_eq!(listed["reason_code"], "repair_listed_fields");
        assert_eq!(listed["basis"]["feedback_pointer"], "/feedback");
        assert!(listed.get("stop").is_none());

        let bare = advise(
            "kmp_ask",
            json!({"isError": true, "structuredContent": {"error": {"code": "invalid_argument"}}}),
        );
        assert_eq!(bare["reason_code"], "repair_arguments");
        assert!(bare.get("stop").is_none());

        let backend = advise(
            "kmp_ask",
            json!({"isError": true, "structuredContent": {"error": {"code": "backend_error"}}}),
        );
        assert_eq!(backend["reason_code"], "operation_refused");
        assert_eq!(backend["stop"], true);
    }
}
