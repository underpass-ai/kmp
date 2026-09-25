//! Whether each standing English search summary says what its memory says.
//!
//! The audit's own reading judges form and never meaning: a fluent summary
//! with the wrong number, actor or polarity clears the lint. On a store that
//! opted into TypeSafe Jev, the page's standing summaries are read against
//! their text, and one Jev does not read as faithful gains the `unfaithful`
//! weakness. The judgement is advice beside the audit, not part of it.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::ports::judgement_model::JudgementModel;
use crate::summaries::SummaryAudit;

/// Below this, Jev does not read the summary as faithful.
const FAITHFUL_AT: f64 = 0.5;
const TEXT_CHARS: usize = 1_500;

/// Reads the page's standing summaries against their memory and marks the
/// ones Jev does not read as faithful. Adds the `jev` usage to the page.
pub(crate) async fn judge_summary_meaning(
    model: &dyn JudgementModel,
    audit: &SummaryAudit,
    page: &mut Value,
) {
    judge_page(
        model,
        |reference| {
            audit
                .entries()
                .iter()
                .find(|entry| entry.reference == reference)
                .map(|entry| entry.source_text.clone())
                .filter(|text| !text.is_empty())
        },
        page,
    )
    .await;
}

async fn judge_page(
    model: &dyn JudgementModel,
    text_of: impl Fn(&str) -> Option<String>,
    page: &mut Value,
) {
    let Some(entries) = page.get_mut("entries").and_then(Value::as_array_mut) else {
        return;
    };
    let mut questions = BTreeMap::new();
    for (n, entry) in entries.iter().enumerate() {
        let (Some(text), Some(summary)) = (
            entry.get("ref").and_then(Value::as_str).and_then(&text_of),
            entry.get("summary_en").and_then(Value::as_str),
        ) else {
            continue;
        };
        if entry.get("state").and_then(Value::as_str) != Some("stands") {
            continue;
        }
        questions.insert(
            format!("f{n}"),
            JudgementQuestion::Noul {
                instructions: json!({
                    "memory": text.chars().take(TEXT_CHARS).collect::<String>(),
                    "summary": summary,
                    "question": "Does `summary` say what `memory` says, in another language perhaps, without changing or adding any fact, number, quantity or polarity?",
                }),
            },
        );
    }
    if questions.is_empty() {
        return;
    }
    let request = JudgementRequest {
        state: json!("English search summaries checked against the memory each renders."),
        questions,
    };
    let response = match model.evaluate(&request).await {
        Ok(response) => response,
        Err(error) => {
            push_warning(page, format!("summary meaning not checked: {error}"));
            return;
        }
    };
    let Some(entries) = page.get_mut("entries").and_then(Value::as_array_mut) else {
        return;
    };
    for (n, entry) in entries.iter_mut().enumerate() {
        let Some(JudgementAnswer::Noul { yes }) = response.answers.get(&format!("f{n}")) else {
            continue;
        };
        if *yes >= FAITHFUL_AT {
            continue;
        }
        let weakness = json!({
            "signal": "unfaithful",
            "says": format!("Jev does not read the summary as saying what the memory says ({yes:.2}); render it again from the text"),
        });
        match entry.get_mut("weaknesses").and_then(Value::as_array_mut) {
            Some(weaknesses) => weaknesses.push(weakness),
            None => entry["weaknesses"] = json!([weakness]),
        }
    }
    page["jev"] = json!({
        "model": response.model,
        "requests": response.requests,
        "input_tokens": response.input_tokens,
    });
}

fn push_warning(page: &mut Value, warning: String) {
    match page.get_mut("warnings").and_then(Value::as_array_mut) {
        Some(warnings) => warnings.push(json!(warning)),
        None => page["warnings"] = json!([warning]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curate::application::use_cases::scripted_judgement::Scripted;
    use std::sync::Mutex;

    fn sample() -> Value {
        json!({"entries": [
            {"ref": "a:1", "state": "stands", "summary_en": "Refunds take a week."},
            {"ref": "a:2", "state": "missing"},
            {"ref": "a:3", "state": "stands", "summary_en": "x y", "weaknesses": [{"signal": "thin"}]}
        ], "warnings": []})
    }

    #[tokio::test]
    async fn standing_summaries_jev_doubts_gain_unfaithful_and_the_rest_are_left() {
        let doubting = Scripted {
            noul: 0.1,
            choice: "none",
            confidence: 0.9,
            calls: Mutex::new(0),
        };
        let mut page = sample();
        judge_page(
            &doubting,
            |_| Some("Las devoluciones tardan 48 horas.".into()),
            &mut page,
        )
        .await;
        assert_eq!(page["entries"][0]["weaknesses"][0]["signal"], "unfaithful");
        assert!(
            page["entries"][1].get("weaknesses").is_none(),
            "missing is not judged"
        );
        assert_eq!(page["entries"][2]["weaknesses"][1]["signal"], "unfaithful");
        assert_eq!(page["jev"]["requests"], 1);

        let trusting = Scripted {
            noul: 0.9,
            choice: "none",
            confidence: 0.9,
            calls: Mutex::new(0),
        };
        let mut page = sample();
        judge_page(&trusting, |_| Some("text".into()), &mut page).await;
        assert!(page["entries"][0].get("weaknesses").is_none());

        let mut untouched = sample();
        judge_page(&trusting, |_| None, &mut untouched).await;
        assert_eq!(
            *trusting.calls.lock().expect("calls"),
            1,
            "no text, no request"
        );
    }
}
