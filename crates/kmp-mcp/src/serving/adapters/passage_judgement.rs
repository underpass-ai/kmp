use kmp_proto_mapping::v1beta1::SemanticSource;
use serde_json::Value;

use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::ports::judgement_model::JudgementModel;

/// One yes/no question about each passage of a pool (`ask` builds it from
/// the passage's excerpt), against a state. The passages `keep` admits by
/// their probability of yes come back best first, as entry ref and text
/// fingerprint, at most `limit` of them; ties break by ref, then by
/// fingerprint, so the order is reproducible.
pub(super) async fn judge_passages(
    model: &dyn JudgementModel,
    state: Value,
    ask: impl Fn(String) -> JudgementQuestion,
    pool: &[SemanticSource],
    excerpt_chars: usize,
    keep: impl Fn(f64) -> bool,
    limit: usize,
) -> Result<Vec<(String, String)>, String> {
    let questions = pool
        .iter()
        .enumerate()
        .map(|(n, source)| {
            (
                format!("p{n}"),
                ask(source.text.chars().take(excerpt_chars).collect::<String>()),
            )
        })
        .collect();
    let response = model
        .evaluate(&JudgementRequest { state, questions })
        .await?;
    let mut scored = pool
        .iter()
        .enumerate()
        .map(|(n, source)| {
            let yes = match response.answers.get(&format!("p{n}")) {
                Some(JudgementAnswer::Noul { yes }) => *yes,
                _ => 0.0,
            };
            (yes, source)
        })
        .filter(|(yes, _)| keep(*yes))
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then_with(|| left.1.entry_ref.cmp(&right.1.entry_ref))
            .then_with(|| left.1.text_sha256.cmp(&right.1.text_sha256))
    });
    Ok(scored
        .into_iter()
        .take(limit)
        .map(|(_, source)| (source.entry_ref.clone(), source.text_sha256.clone()))
        .collect())
}
