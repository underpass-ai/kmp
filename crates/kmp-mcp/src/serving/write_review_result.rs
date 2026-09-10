use crate::write::plan::KernelWritePlan;
use serde_json::{Value, json};

pub(super) fn pending_review(
    arguments: &Value,
    plan: &KernelWritePlan,
    mut neighborhood: Value,
) -> Value {
    let mut resume = arguments.clone();
    resume["review_token"] = neighborhood["token"].clone();
    resume["idempotency_key"] = plan.ingest_arguments["idempotency_key"].clone();
    for item in neighborhood["items"].as_array_mut().into_iter().flatten() {
        if item["state"] == "stored" {
            item["action"] = json!({"tool":"kmp_inspect","arguments":{
                "about":item["about"],"ref":item["ref"],
                "include":{"details":true,"incoming":true,"outgoing":true,"raw":false}
            }});
        }
    }
    let expand = neighborhood["abouts"].as_array().into_iter().flatten().map(|about| json!({
        "tool":"kmp_wake","arguments":{"about":about,"budget":{"detail":"full","max_bytes":10000}}
    })).collect::<Vec<_>>();
    json!({
        "accepted":false,"status":"needs_review","dry_run":plan.dry_run,
        "summary":"Nothing written. Review stored context and proposed link directions. Resume the unchanged packet, or correct it and submit again. A compact view is not complete evidence; expand omissions when needed.",
        "neighborhood":neighborhood,
        "relations":crate::write::relation_triples(plan),
        "local_refs":plan.local_refs,
        "next_actions": [json!({"tool":"kmp_write_memory","arguments":resume})],
        "expand_context": expand,
    })
}
