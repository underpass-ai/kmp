use std::collections::{BTreeMap, BTreeSet};

use kmp_proto_mapping::v1beta1::PartnerShortlist;
use serde_json::json;

use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::judgement_plan::{excerpt, text_of};
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;

/// Most facts a focused review asks about in one request.
pub(crate) const FOCUS_FACTS: usize = 8;
/// The best BM25 matches of a focused fact read beside the facts that share
/// a hard anchor with it (DESIGN L4 4e). About 80 tokens per question, so a
/// fact costs about 3.2k tokens however large its about grows.
pub(crate) const LEXICAL_PARTNERS: usize = 40;
const FOCUS_CHARS: usize = 1_000;
const CANDIDATE_CHARS: usize = 300;

/// For each focused fact (`new.k<k>`) and each of its shortlisted partners:
/// is there a direct relation between them (`r<k>_<n>`)? The shortlist is
/// the current facts of its about that share a hard anchor with it and its
/// best BM25 matches ([`PartnerShortlist`]), asked in the about's order; an
/// about of at most [`LEXICAL_PARTNERS`] other facts is read whole. Returns
/// the request and, per key, the (focused, other) refs it asks about.
pub(crate) fn related_request(
    material: &CurateMaterial,
    focus: &[String],
) -> (JudgementRequest, BTreeMap<String, (String, String)>) {
    let mut state = serde_json::Map::new();
    let mut questions = BTreeMap::new();
    let mut keys = BTreeMap::new();
    let mut shortlists = BTreeMap::new();
    for (k, focused) in focus.iter().take(FOCUS_FACTS).enumerate() {
        let Some(fact) = material.fact(focused) else {
            continue;
        };
        state.insert(
            format!("k{k}"),
            json!(excerpt(&text_of(material, focused), FOCUS_CHARS)),
        );
        let about = shortlists.entry(fact.about.clone()).or_insert_with(|| {
            PartnerShortlist::over(
                material
                    .facts
                    .iter()
                    .filter(|other| other.about == fact.about)
                    .map(|other| (other.reference.as_str(), other.text.as_str())),
            )
        });
        let partners = about.partners(focused, &fact.text, LEXICAL_PARTNERS);
        let shortlisted = partners.refs().collect::<BTreeSet<_>>();
        let others = material.facts.iter().filter(|other| {
            other.about == fact.about
                && !focus.contains(&other.reference)
                && shortlisted.contains(&other.reference)
        });
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curate::domain::curate_fact::CurateFact;

    fn material(facts: Vec<(String, String)>) -> CurateMaterial {
        CurateMaterial {
            facts: facts
                .into_iter()
                .map(|(reference, text)| CurateFact {
                    reference,
                    about: "a".into(),
                    kind: String::new(),
                    text,
                    occurred: None,
                    labels: Vec::new(),
                })
                .collect(),
            declared: Vec::new(),
            pairs: Vec::new(),
            selection: "fp".into(),
            past: Vec::new(),
        }
    }

    fn others(keys: &BTreeMap<String, (String, String)>) -> Vec<&str> {
        keys.values().map(|(_, other)| other.as_str()).collect()
    }

    #[test]
    fn a_small_about_is_read_whole_in_its_own_order() {
        let facts = (0..10)
            .map(|n| (format!("f{n}"), format!("note number {n} about lunch")))
            .chain([("new".to_string(), "Valkey failover".to_string())])
            .collect();
        let (request, keys) = related_request(&material(facts), &["new".to_string()]);
        assert_eq!(request.questions.len(), 10);
        assert_eq!(keys["r0_0"].1, "f0", "the about's order, not the score's");
        assert_eq!(keys["r0_9"].1, "f9");
    }

    #[test]
    fn a_large_about_is_read_through_the_shortlist_and_keeps_anchored_facts() {
        let mut facts = (0..300)
            .map(|n| {
                (
                    format!("f{n}"),
                    format!("routine note {n} about the office lunch"),
                )
            })
            .collect::<Vec<_>>();
        facts.push((
            "paraphrase".into(),
            "Ticket INC-4711 closed after the review.".into(),
        ));
        facts.push(("bm25".into(), "The Valkey cache failover drill.".into()));
        facts.push((
            "new".into(),
            "INC-4711: the Valkey cache failover dropped payments.".into(),
        ));
        let (request, keys) = related_request(&material(facts), &["new".to_string()]);
        assert!(
            request.questions.len() <= LEXICAL_PARTNERS + 1,
            "{} questions",
            request.questions.len()
        );
        let asked = others(&keys);
        assert!(
            asked.contains(&"paraphrase"),
            "shares the anchor: {asked:?}"
        );
        assert!(asked.contains(&"bm25"), "best lexical match: {asked:?}");
    }
}
