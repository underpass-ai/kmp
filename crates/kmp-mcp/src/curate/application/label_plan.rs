use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;

use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::judgement_plan::{excerpt, text_of};
use crate::curate::domain::curate_thresholds::NONE;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;

const EXAMPLES_PER_VALUE: usize = 2;
const EXAMPLE_CHARS: usize = 160;
const FACT_CHARS: usize = 600;

/// The values each label key takes in `about`, with a few facts that carry
/// each one: the catalogue the judge chooses from, shown by example.
pub(crate) fn catalogue(
    material: &CurateMaterial,
    about: &str,
) -> BTreeMap<String, BTreeMap<String, Vec<String>>> {
    let mut catalogue = BTreeMap::<String, BTreeMap<String, Vec<String>>>::new();
    for fact in material
        .facts
        .iter()
        .chain(&material.past)
        .filter(|fact| fact.about == about)
    {
        for (key, value) in &fact.labels {
            let examples = catalogue
                .entry(key.clone())
                .or_default()
                .entry(value.clone())
                .or_default();
            if examples.len() < EXAMPLES_PER_VALUE {
                examples.push(excerpt(&fact.text, EXAMPLE_CHARS));
            }
        }
    }
    catalogue
}

/// For each focused fact and each key of its about's catalogue it has no
/// value under, a choice among that key's values or none (`l<k>_<j>`). A key
/// with a single value is skipped: it separates nothing. Returns the
/// request and, per question key, the (fact, label key) it asks about.
pub(crate) fn label_request(
    material: &CurateMaterial,
    focus: &[String],
) -> (JudgementRequest, BTreeMap<String, (String, String)>) {
    let mut questions = BTreeMap::new();
    let mut keys = BTreeMap::new();
    let mut shown = BTreeMap::<String, serde_json::Value>::new();
    for (k, reference) in focus.iter().enumerate() {
        let Some(fact) = material.fact(reference) else {
            continue;
        };
        let held = fact
            .labels
            .iter()
            .map(|(key, _)| key.as_str())
            .collect::<BTreeSet<_>>();
        let catalogue = catalogue(material, &fact.about);
        for (j, (key, values)) in catalogue
            .iter()
            .filter(|(key, values)| !held.contains(key.as_str()) && values.len() > 1)
            .enumerate()
        {
            shown.insert(key.clone(), json!(values));
            let question = format!("l{k}_{j}");
            questions.insert(
                question.clone(),
                JudgementQuestion::Choice {
                    instructions: json!({
                        "memory": excerpt(&text_of(material, reference), FACT_CHARS),
                        "question": format!(
                            "Which `{key}` does `memory` belong to, judging by the examples of each value in the catalogue? Answer none when none fits."
                        ),
                    }),
                    options: values
                        .keys()
                        .cloned()
                        .chain(std::iter::once(NONE.to_string()))
                        .collect(),
                },
            );
            keys.insert(question, (reference.clone(), key.clone()));
        }
    }
    (
        JudgementRequest {
            state: json!({ "catalogue": shown }),
            questions,
        },
        keys,
    )
}
