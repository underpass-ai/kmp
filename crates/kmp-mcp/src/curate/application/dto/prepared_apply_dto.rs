use serde_json::{Value, json};

use crate::curate::application::prepared_apply::PreparedApply;
use crate::curate::domain::jev_verdict::JevVerdict;

/// The backend's answer to `prepare_apply`, read by the apply dispatcher.
pub(crate) fn prepared_to_value(prepared: &PreparedApply) -> Value {
    json!({
        "relations": prepared.relations.iter().map(|relation| json!({
            "item_id": relation.item_id,
            "from": relation.from,
            "to": relation.to,
            "rel": relation.rel,
            "why": relation.why,
            "evidence": relation.evidence,
            "confidence": relation.confidence,
            "proposal": relation.proposal,
            "proposed_by": relation.origin.name(),
        })).collect::<Vec<_>>(),
        "doubted": prepared.doubted.iter().map(|doubt| json!({
            "item_id": doubt.item_id,
            "support": doubt.support,
            "suggested_rel": doubt.best.choice,
            "jev": verdict(&doubt.best),
        })).collect::<Vec<_>>(),
        "rejected": prepared.rejected.iter().map(|rejection| json!({
            "item_id": rejection.item_id, "reason": rejection.reason,
        })).collect::<Vec<_>>(),
        "jev": prepared.jev.as_ref().map(|usage| json!({
            "model": usage.model, "requests": usage.requests, "input_tokens": usage.input_tokens,
        })),
        "checked_by": prepared.checked_by,
        "warnings": prepared.warnings,
    })
}

fn verdict(verdict: &JevVerdict) -> Value {
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
    use crate::curate::application::prepared_relation::PreparedRelation;
    use crate::curate::domain::apply_rejection::ApplyRejection;
    use crate::curate::domain::pair_origin::PairOrigin;

    #[test]
    fn relations_rejections_and_usage_are_carried_verbatim() {
        let value = prepared_to_value(&PreparedApply {
            relations: vec![PreparedRelation {
                item_id: "m0".into(),
                from: "a1".into(),
                to: "a2".into(),
                rel: "supports".into(),
                why: "w".into(),
                evidence: "e".into(),
                confidence: None,
                proposal: None,
                origin: PairOrigin::Jev,
            }],
            doubted: vec![],
            rejected: vec![ApplyRejection {
                item_id: "m9".into(),
                reason: "r".into(),
            }],
            jev: None,
            checked_by: None,
            warnings: vec![],
        });
        assert_eq!(value["relations"][0]["proposed_by"], "jev");
        assert!(value["relations"][0]["proposal"].is_null());
        assert_eq!(value["rejected"][0]["item_id"], "m9");
        assert!(value["jev"].is_null());
    }
}
