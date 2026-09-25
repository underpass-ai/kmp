use serde_json::{Value, json};

use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::curate_review::CurateReview;
use crate::curate::application::judgement_plan::excerpt;
use crate::curate::domain::curate_finding::CurateFinding;
use crate::curate::domain::jev_verdict::JevVerdict;
use crate::curate::domain::pair_origin::PairOrigin;

const EXCERPT_CHARS: usize = 160;

/// The review as the tool returns it: one page of `missing ++ suspect`,
/// numbered over the whole review so an id means the same thing on every
/// page and in `apply`.
pub(crate) fn review_to_value(
    review: &CurateReview,
    material: &CurateMaterial,
    about: &str,
    token: &str,
    offset: usize,
    entries: usize,
) -> Value {
    let numbered = review.numbered();
    let missing_n = numbered
        .iter()
        .filter(|(_, f)| matches!(f, CurateFinding::Missing { .. }))
        .count();
    let suspect_n = numbered.len() - missing_n;
    let mut ordered = numbered
        .iter()
        .filter(|(_, f)| matches!(f, CurateFinding::Missing { .. }))
        .chain(
            numbered
                .iter()
                .filter(|(_, f)| matches!(f, CurateFinding::Suspect { .. })),
        )
        .collect::<Vec<_>>();
    let total = ordered.len();
    let start = offset.min(total);
    let end = start.saturating_add(entries).min(total);
    let page = ordered.drain(start..end).collect::<Vec<_>>();
    let mut missing = Vec::new();
    let mut suspect = Vec::new();
    for (id, finding) in page {
        match finding {
            CurateFinding::Missing {
                pair,
                suggested_rel,
                verdict,
            } => missing.push(json!({
                "item_id": id,
                "from": side(material, &pair.from),
                "to": side(material, &pair.to),
                "suggested_rel": suggested_rel,
                "proposed_by": pair.origin.name(),
                "signals": match &pair.origin {
                    PairOrigin::Kernel { signals, .. } => json!(signals),
                    PairOrigin::Jev => json!([]),
                },
                "pairing_why": match &pair.origin {
                    PairOrigin::Kernel { why, .. } => json!(why),
                    PairOrigin::Jev => Value::Null,
                },
                "jev": verdict.as_ref().map(jev).unwrap_or(Value::Null),
            })),
            CurateFinding::Suspect {
                link,
                support,
                best,
                direction,
                reasons,
            } => suspect.push(json!({
                "item_id": id,
                "from": side(material, &link.from),
                "to": side(material, &link.to),
                "rel": link.rel,
                "support": support,
                "direction": direction,
                "reasons": reasons,
                "suggested_rel": best.choice,
                "jev": jev(best),
            })),
        }
    }
    let next_cursor = (end < total).then(|| end.to_string());
    let next_actions = next_cursor
        .as_ref()
        .map(|cursor| {
            vec![json!({"tool": "kmp_curate", "arguments": {
                "mode": "review", "about": about, "review_token": token,
                "page": {"entries": entries, "cursor": cursor}}})]
        })
        .unwrap_or_default();
    let jev_used = review.jev.as_ref().map_or_else(
        || "Jev not used".to_string(),
        |usage| format!("Jev {} used {} requests", usage.model, usage.requests),
    );
    json!({
        "summary": format!(
            "{missing_n} missing and {suspect_n} suspect relations in {} facts; {jev_used}",
            material.facts.len()
        ),
        "review_token": token,
        "missing": missing,
        "suspect": suspect,
        "jev": review.jev.as_ref().map(|usage| json!({
            "model": usage.model, "requests": usage.requests, "input_tokens": usage.input_tokens,
        })),
        "page": {"entries": entries, "total": total, "next_cursor": next_cursor},
        "next_actions": next_actions,
        "warnings": review.warnings,
    })
}

fn side(material: &CurateMaterial, reference: &str) -> Value {
    let fact = material.fact(reference);
    json!({
        "ref": reference,
        "about": fact.map(|f| f.about.as_str()).unwrap_or_default(),
        "excerpt": fact.map(|f| excerpt(&f.text, EXCERPT_CHARS)).unwrap_or_default(),
    })
}

fn jev(verdict: &JevVerdict) -> Value {
    let mut top = verdict
        .probabilities
        .iter()
        .map(|(option, p)| (option.clone(), *p))
        .collect::<Vec<_>>();
    top.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.0.cmp(&right.0))
    });
    top.truncate(3);
    json!({"confidence": verdict.confidence, "top": top})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::curate::domain::candidate_pair::CandidatePair;
    use crate::curate::domain::curate_fact::CurateFact;
    use crate::curate::domain::declared_link::DeclaredLink;

    fn material() -> CurateMaterial {
        CurateMaterial {
            facts: ["a1", "a2", "b1"]
                .iter()
                .map(|r| CurateFact {
                    reference: (*r).into(),
                    about: r[..1].into(),
                    text: "x".repeat(400),
                    occurred: None,
                })
                .collect(),
            declared: vec![],
            pairs: vec![],
            selection: "fp".into(),
        }
    }

    fn verdict() -> JevVerdict {
        JevVerdict {
            choice: "supports".into(),
            probabilities: BTreeMap::from([
                ("causes".into(), 0.05),
                ("none".into(), 0.05),
                ("supports".into(), 0.8),
                ("answers".into(), 0.1),
            ]),
            confidence: 0.8,
        }
    }

    fn review() -> CurateReview {
        let missing = |from: &str, to: &str| CurateFinding::Missing {
            pair: CandidatePair {
                from: from.into(),
                to: to.into(),
                origin: PairOrigin::Jev,
                crosses_abouts: false,
            },
            suggested_rel: Some("supports".into()),
            verdict: Some(verdict()),
        };
        CurateReview {
            findings: vec![
                missing("a1", "a2"),
                CurateFinding::Suspect {
                    link: DeclaredLink {
                        from: "a2".into(),
                        to: "b1".into(),
                        rel: "causes".into(),
                        why: "w".into(),
                        evidence: "e".into(),
                    },
                    support: 0.1,
                    best: verdict(),
                    direction: None,
                    reasons: vec!["support"],
                },
                missing("a2", "a1"),
            ],
            jev: None,
            warnings: vec![],
            selection: "fp".into(),
        }
    }

    #[test]
    fn ids_are_stable_across_pages() {
        let first = review_to_value(&review(), &material(), "a", "tok", 0, 2);
        let second = review_to_value(&review(), &material(), "a", "tok", 2, 2);
        assert_eq!(first["missing"][0]["item_id"], "m0");
        assert_eq!(first["missing"][1]["item_id"], "m1");
        assert_eq!(second["suspect"][0]["item_id"], "s0");
        assert_eq!(first["page"]["next_cursor"], "2");
        assert_eq!(first["next_actions"][0]["arguments"]["review_token"], "tok");
        assert_eq!(
            first["missing"][0]["from"]["excerpt"]
                .as_str()
                .expect("excerpt")
                .len(),
            160
        );
    }

    #[test]
    fn top_three_are_sorted() {
        let value = review_to_value(&review(), &material(), "a", "tok", 0, 1);
        assert_eq!(
            value["missing"][0]["jev"]["top"],
            json!([["supports", 0.8], ["answers", 0.1], ["causes", 0.05]])
        );
    }

    #[test]
    fn a_last_page_has_no_next_action() {
        let value = review_to_value(&review(), &material(), "a", "tok", 2, 8);
        assert!(value["page"]["next_cursor"].is_null());
        assert_eq!(value["next_actions"], json!([]));
        assert!(
            value["summary"]
                .as_str()
                .expect("summary")
                .contains("Jev not used")
        );
    }
}
