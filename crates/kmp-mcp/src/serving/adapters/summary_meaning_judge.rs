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
use crate::serving::noul_criteria::NoulCriteria;
use crate::serving::noul_reading::NoulReading;
use crate::serving::ports::judgement_model::JudgementModel;
use crate::summaries::SummaryAudit;

const TEXT_CHARS: usize = 1_500;

/// One judgement per question (TypeSafe's guide): each part asks one way a
/// summary can say something its memory does not, with what a yes and a no
/// mean. A yes to any part marks the summary unfaithful; a no to every part
/// leaves it alone; anything between is unsettled, and is marked too, with
/// its own words. Changing a meaning bumps `JudgementSite::Summaries`.
const PARTS: [(&str, &str, &str, &str); 3] = [
    (
        "changed",
        "Does `summary` give a different fact, name, number or quantity than `memory` gives?",
        "the summary changes something the memory states, such as 'weekly' where the memory says 'daily', or 'some' where it says 'all'",
        "every fact, name, number and quantity in the summary matches the memory, perhaps in another language",
    ),
    (
        "added",
        "Does `summary` state a fact that `memory` does not state?",
        "the summary adds a fact, name, number or claim that is absent from the memory",
        "everything the summary says is in the memory, though it may leave things out",
    ),
    (
        "negated",
        "Does `summary` reverse whether something happened or holds, compared with `memory`?",
        "the summary drops or adds a negation, such as 'the deploy passed' where the memory says it failed",
        "the summary affirms and denies the same things as the memory",
    ),
];

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
        let memory = text.chars().take(TEXT_CHARS).collect::<String>();
        for (part, question, when_true, when_false) in PARTS {
            questions.insert(
                format!("f{n}_{part}"),
                JudgementQuestion::CriteriaNoul {
                    instructions: json!({
                        "memory": memory,
                        "summary": summary,
                        "question": question,
                    }),
                    criteria: NoulCriteria::new(when_true, when_false),
                },
            );
        }
    }
    if questions.is_empty() {
        return;
    }
    let request = JudgementRequest {
        state: json!({}),
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
        let parts = PARTS
            .iter()
            .map(
                |(part, ..)| match response.answers.get(&format!("f{n}_{part}")) {
                    Some(JudgementAnswer::Noul { yes }) => Some((*part, *yes)),
                    _ => None,
                },
            )
            .collect::<Option<Vec<_>>>();
        // Not asked, or not answered: not judged, never faithful by default.
        let Some(parts) = parts else {
            continue;
        };
        let says = if let Some((part, yes)) = parts
            .iter()
            .find(|(_, yes)| NoulReading::of(*yes) == NoulReading::Yes)
        {
            format!(
                "Jev reads the summary as not saying what the memory says ({}: {yes:.2}); render it again from the text",
                part_words(part)
            )
        } else if parts
            .iter()
            .all(|(_, yes)| NoulReading::of(*yes) == NoulReading::No)
        {
            continue;
        } else {
            let highest = parts.iter().map(|(_, yes)| *yes).fold(0.0_f64, f64::max);
            format!(
                "Jev cannot settle whether the summary says what the memory says ({highest:.2}); check it against the text"
            )
        };
        let weakness = json!({"signal": "unfaithful", "says": says});
        match entry.get_mut("weaknesses").and_then(Value::as_array_mut) {
            Some(weaknesses) => weaknesses.push(weakness),
            None => entry["weaknesses"] = json!([weakness]),
        }
    }
    page["jev"] = json!({
        "model": response.model,
        "requests": response.requests,
        "input_tokens": response.input_tokens,
        "elapsed_ms": response.elapsed_us / 1_000,
    });
}

fn part_words(part: &str) -> &'static str {
    match part {
        "changed" => "a fact, name, number or quantity changed",
        "added" => "a fact added",
        _ => "a negation dropped or added",
    }
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

    fn every_part_at(noul: f64) -> Scripted {
        Scripted {
            noul,
            choice: "none",
            confidence: 0.9,
            calls: Mutex::new(0),
        }
    }

    #[tokio::test]
    async fn a_yes_to_any_part_marks_a_standing_summary_unfaithful() {
        let doubting = every_part_at(0.9);
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
    }

    #[tokio::test]
    async fn a_no_to_every_part_leaves_it_and_between_is_unsettled() {
        let trusting = every_part_at(0.1);
        let mut page = sample();
        judge_page(&trusting, |_| Some("text".into()), &mut page).await;
        assert!(page["entries"][0].get("weaknesses").is_none());

        let unsure = every_part_at(0.5);
        let mut page = sample();
        judge_page(&unsure, |_| Some("text".into()), &mut page).await;
        let says = page["entries"][0]["weaknesses"][0]["says"]
            .as_str()
            .expect("says");
        assert!(says.contains("cannot settle"), "{says}");

        let mut untouched = sample();
        judge_page(&trusting, |_| None, &mut untouched).await;
        assert_eq!(
            *trusting.calls.lock().expect("calls"),
            1,
            "no text, no request"
        );
    }

    #[tokio::test]
    async fn each_part_is_one_question_with_criteria_over_an_empty_state() {
        struct Recording(Mutex<Option<JudgementRequest>>);
        impl JudgementModel for Recording {
            fn model(&self) -> &str {
                "jev-test"
            }
            fn evaluate<'a>(
                &'a self,
                request: &'a JudgementRequest,
            ) -> std::pin::Pin<
                Box<
                    dyn std::future::Future<
                            Output = Result<
                                crate::serving::judgement_response::JudgementResponse,
                                String,
                            >,
                        > + Send
                        + 'a,
                >,
            > {
                *self.0.lock().expect("lock") = Some(request.clone());
                Box::pin(async { Err("not answered".to_string()) })
            }
        }
        let model = Recording(Mutex::new(None));
        let mut page = sample();
        judge_page(&model, |_| Some("text".into()), &mut page).await;
        let request = model.0.lock().expect("lock").clone().expect("asked");
        assert_eq!(request.state, json!({}));
        assert_eq!(
            request.questions.len(),
            6,
            "three parts for each of two summaries"
        );
        for question in request.questions.values() {
            let JudgementQuestion::CriteriaNoul { instructions, .. } = question else {
                panic!("a noul with criteria");
            };
            assert!(
                !instructions["question"]
                    .as_str()
                    .unwrap_or("")
                    .contains(" or polarity")
            );
        }
        assert!(
            page["entries"][0].get("weaknesses").is_none(),
            "a failed judgement is not judged, never unfaithful"
        );
        assert!(
            page["warnings"][0]
                .as_str()
                .unwrap_or("")
                .contains("not checked")
        );
    }
}
