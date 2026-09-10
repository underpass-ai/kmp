use super::KernelMcpServer;
use super::call_guidance::CallGuidance;
use super::guidance_recommendation::GuidanceRecommendation;
use super::tool_error_help::ToolErrorHelp;
use crate::guidance::{AgentOpen, UseOutcome};
use serde_json::{Value, json};

impl KernelMcpServer {
    pub(super) fn complete_work_guidance(
        &self,
        tool: &str,
        arguments: &Value,
        call: &CallGuidance,
        response: String,
    ) -> String {
        let Ok(mut envelope) = serde_json::from_str::<Value>(&response) else {
            return response;
        };
        let Some(result) = envelope.get_mut("result") else {
            return response;
        };
        let body = &result["structuredContent"];
        let outcome = if result["isError"] == true {
            UseOutcome::Rejected
        } else if tool == "kmp_ask" && body["answer"] == "UNKNOWN" {
            UseOutcome::Unknown
        } else {
            UseOutcome::Returned
        };
        let guide_read = tool == "kmp_inspect" && arguments["about"] == "guide:kmp-agent";
        let mut delivered_context = None;
        let observed = self.guidance_directory(true).and_then(|directory| {
            if guide_read
                && matches!(outcome, UseOutcome::Returned)
                && body["object"]["text"].is_string()
                && let (Some(reference), Some(revision)) = (
                    body["object"]["ref"].as_str(),
                    body["object"]["metadata"]["guide_revision"]
                        .as_str()
                        .filter(|value| !value.is_empty()),
                )
            {
                // Only an actual returned body counts. A reused object or a
                // suggested example is not delivery of that example.
                if revision != call.context.guide_revision {
                    delivered_context = Some(
                        directory
                            .open(
                                &AgentOpen::Resume {
                                    context: call.context.session.context_id.clone(),
                                },
                                revision,
                            )
                            .map_err(|e| super::guide_dispatch::guidance_error(&e))?,
                    );
                }
                delivered_context = Some(
                    directory
                        .served(&call.context.session, revision, reference)
                        .map_err(|e| super::guide_dispatch::guidance_error(&e))?,
                );
            }
            let context = delivered_context.as_ref().unwrap_or(&call.context);
            directory
                .record_use(&context.session, &context.guide_revision, tool, outcome)
                .map_err(|e| super::guide_dispatch::guidance_error(&e))
        });
        let context = delivered_context.as_ref().unwrap_or(&call.context);
        let usage = match &observed {
            Ok(usage) => {
                json!({"recorded":true,"attempts":usage.attempts,"rejected":usage.rejected,"unknown":usage.unknown})
            }
            // Never turn an accepted memory write into a failure of ancillary metadata.
            Err(error) => {
                json!({"recorded":false,"error_code":error.code.as_str(),"message":"Usage metadata could not be recorded. Keep the original operation result; do not repeat a write for this warning."})
            }
        };
        let mut recommendation =
            GuidanceRecommendation::choose(call.purpose, tool, arguments, result);
        if let Some(advice) = recommendation.as_mut()
            && let Some(action) = advice.get_mut("action")
        {
            *action = call.contextualize(action.clone());
        }
        let topic = ToolErrorHelp::topic(tool);
        let served = topic.is_some_and(|topic| {
            context.served.iter().any(|seen| {
                seen == topic
                    || seen == &format!("guide:kmp-agent:verb:{topic}")
                    || seen == &format!("guide:kmp-agent:card:{topic}")
            })
        });
        let first = observed.as_ref().is_ok_and(|usage| usage.attempts == 1);
        let needs_help =
            !guide_read && !served && (first || !matches!(outcome, UseOutcome::Returned));
        let help = topic.filter(|_| needs_help).map(|topic| {
            call.contextualize(json!({"tool":"kmp_guide","arguments":{"topic":topic}}))
        });
        let partial = body.pointer("/page/has_more") == Some(&Value::Bool(true))
            || body.pointer("/projection/page/has_more") == Some(&Value::Bool(true))
            || body.pointer("/projection/core_text_shortened") == Some(&Value::Bool(true));
        let partial = partial || body.pointer("/neighborhood/partial") == Some(&Value::Bool(true));
        let mut signals = json!({"packet_partial":partial});
        if body["status"] == "needs_review" {
            signals["needs_review"] = json!(true);
        }
        if let Some(selection) = body.get("selection") {
            signals["selection"] = selection.clone();
        }
        if let Some(coverage) = body.get("coverage") {
            signals["coverage"] = coverage.clone();
        }
        if let Some(accepted) = body.get("accepted") {
            signals["accepted"] = accepted.clone();
        }
        if let Some(preview) = body.get("dry_run") {
            signals["dry_run"] = preview.clone();
        }
        let guidance = json!({
            "context_id":call.context.session.context_id.as_str(),
            "guide_revision_seen":context.guide_revision,
            "purpose":call.purpose.as_str(),"usage":usage,
            "topic_served_in_context":served,"help":help,"recommendation":recommendation,"signals":signals
        });
        // One advisory block, outside structured memory's byte budget. Do not
        // duplicate it in _meta or rewrite the original proof/receipt envelope.
        if let Some(content) = result.get_mut("content").and_then(Value::as_array_mut) {
            content
                .push(json!({"type":"text","text":json!({"kmp_guidance":guidance}).to_string()}));
        }
        envelope.to_string()
    }
}
