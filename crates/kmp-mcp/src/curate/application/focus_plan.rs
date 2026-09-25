use std::collections::BTreeMap;

use serde_json::json;

use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::judgement_plan::{excerpt, text_of};
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;

/// Most facts a focused review asks about in one request.
pub(crate) const FOCUS_FACTS: usize = 8;
const FOCUS_CHARS: usize = 1_000;
const CANDIDATE_CHARS: usize = 300;

/// For each focused fact (`new.k<k>`) and each other current fact of its
/// about: is there a direct relation between them (`r<k>_<n>`)? The whole
/// about is read, however large, one short question per fact. Returns the
/// request and, per key, the (focused, other) refs it asks about.
pub(crate) fn related_request(
    material: &CurateMaterial,
    focus: &[String],
) -> (JudgementRequest, BTreeMap<String, (String, String)>) {
    let mut state = serde_json::Map::new();
    let mut questions = BTreeMap::new();
    let mut keys = BTreeMap::new();
    for (k, focused) in focus.iter().take(FOCUS_FACTS).enumerate() {
        let Some(fact) = material.fact(focused) else {
            continue;
        };
        state.insert(
            format!("k{k}"),
            json!(excerpt(&text_of(material, focused), FOCUS_CHARS)),
        );
        let others = material
            .facts
            .iter()
            .filter(|other| other.about == fact.about && !focus.contains(&other.reference));
        for (n, other) in others.enumerate() {
            let key = format!("r{k}_{n}");
            questions.insert(
                key.clone(),
                JudgementQuestion::Noul {
                    instructions: json!({
                        "passage": excerpt(&text_of(material, &other.reference), CANDIDATE_CHARS),
                        "question": format!(
                            "Is there a direct relation between `passage` and `new.k{k}`: one causes, explains, supports, contradicts, updates, answers or repeats the other?"
                        ),
                    }),
                },
            );
            keys.insert(key, (focused.clone(), other.reference.clone()));
        }
    }
    (
        JudgementRequest {
            state: json!({ "new": state }),
            questions,
        },
        keys,
    )
}
