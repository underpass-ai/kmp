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
            return Some(
                json!({"reason_code":"operation_refused","reason":"Keep the original error and feedback; do not infer a repair from wording.","basis":{"error_code":body["error"]["code"]},"stop":true}),
            );
        }
        let partial = body.pointer("/page/has_more") == Some(&Value::Bool(true))
            || body.pointer("/projection/page/has_more") == Some(&Value::Bool(true))
            || body.pointer("/projection/core_text_shortened") == Some(&Value::Bool(true));
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
