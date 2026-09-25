use serde_json::{Value, json};

use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::curate_review::CurateReview;
use crate::curate::application::judgement_plan::excerpt;
use crate::curate::application::path_search::PathSearch;
use crate::curate::domain::curate_finding::CurateFinding;

const EXCERPT_CHARS: usize = 160;

/// Paths as the tool returns them. A proposed hop carries the `item_id` it
/// has in the frozen review, so `mode: apply` can declare it.
pub(crate) fn paths_to_value(
    search: &PathSearch,
    review: &CurateReview,
    material: &CurateMaterial,
    token: &str,
) -> Value {
    let item_of = |from: &str, to: &str| {
        review
            .numbered()
            .into_iter()
            .find_map(|(id, finding)| match finding {
                CurateFinding::Missing { pair, .. }
                    if (pair.from == from && pair.to == to)
                        || (pair.from == to && pair.to == from) =>
                {
                    Some(id)
                }
                _ => None,
            })
    };
    let side = |reference: &str| {
        let fact = material.fact(reference);
        json!({
            "ref": reference,
            "about": fact.map(|f| f.about.as_str()).unwrap_or_default(),
            "excerpt": fact.map(|f| excerpt(&f.text, EXCERPT_CHARS)).unwrap_or_default(),
        })
    };
    let paths = search
        .paths
        .iter()
        .map(|path| {
            json!({
                "hops": path.hops.iter().map(|hop| json!({
                    "from": side(&hop.from),
                    "to": side(&hop.to),
                    "rel": hop.rel,
                    "declared": hop.declared,
                    "reversed": hop.reversed,
                    "confidence": hop.confidence,
                    "item_id": if hop.declared { None } else { item_of(&hop.from, &hop.to) },
                })).collect::<Vec<_>>(),
                "proposed": path.proposed(),
                "confidence": path.confidence(),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "summary": format!(
            "{} paths; {} of {} facts judged on the way; {} declared relations avoided; {}",
            search.paths.len(),
            search.kept,
            search.considered,
            search.avoided.len(),
            search.jev.as_ref().map_or_else(
                || "declared relations only".to_string(),
                |usage| format!("Jev {} used {} requests", usage.model, usage.requests)
            )
        ),
        "review_token": token,
        "paths": paths,
        "avoided": search.avoided.iter().map(|avoided| json!({
            "from": side(&avoided.hop.from),
            "to": side(&avoided.hop.to),
            "rel": avoided.hop.rel,
            "support": avoided.support,
        })).collect::<Vec<_>>(),
        "jev": search.jev.as_ref().map(|usage| json!({
            "model": usage.model, "requests": usage.requests, "input_tokens": usage.input_tokens,
        })),
        "next_actions": [],
        "warnings": search.warnings,
    })
}
