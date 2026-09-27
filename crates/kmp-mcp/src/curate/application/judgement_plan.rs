use std::collections::BTreeMap;

use kmp_domain::{KnownMemoryRelationType, MemoryRelationType};
use serde_json::json;

use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::prepared_relation::PreparedRelation;
use crate::curate::domain::candidate_pair::CandidatePair;
use crate::curate::domain::curate_thresholds::{NONE, SYMMETRIC};
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;

const SENT_CHARS: usize = 2_000;
/// Chars of each text a confirming question reads, as the partner round
/// read them.
const CONFIRM_CHARS: usize = 400;

pub(crate) fn excerpt(text: &str, chars: usize) -> String {
    text.chars().take(chars).collect()
}

/// Which way an item about to be written runs, asked as a choice between two concrete
/// sentences over the two dated texts alone. The author's why is left out on
/// purpose: measured on a reversed declaration, a wrong why moved the judge
/// from 0.01 to 0.86 towards the declared direction. Asked only before a
/// write: auditing stored declarations with it flagged nothing more in three
/// recorded samples (jev-evaluation.md), so the audit does not pay for it.
fn direction_question(from: &str, to: &str, relation: &str) -> Option<JudgementQuestion> {
    if SYMMETRIC.contains(&relation) {
        return None;
    }
    let verb = relation_verb(relation);
    Some(JudgementQuestion::Choice {
        instructions: json!({
            "a": excerpt(from, SENT_CHARS),
            "b": excerpt(to, SENT_CHARS),
            "question": format!(
                "Which is true? forward: `a` {verb} `b`. backward: `b` {verb} `a`. neither: neither holds."
            ),
        }),
        options: vec!["forward".into(), "backward".into(), NONE.into()],
    })
}

/// A relation as a verb phrase a reader can test in both directions.
fn relation_verb(relation: &str) -> String {
    match relation {
        "supersedes" => "replaces".into(),
        "updates_state" => "reports a later state of what is described in".into(),
        "supports" => "gives evidence for".into(),
        "chosen_because" => "was decided because of".into(),
        "triggers" => "caused".into(),
        "depends_on" => "depends on".into(),
        "verified_by" => "is confirmed by".into(),
        "corrects" => "corrects".into(),
        "derived_from" => "is derived from".into(),
        "satisfies_constraint" => "complies with".into(),
        "violates_constraint" => "breaks".into(),
        "follows" => "comes after".into(),
        "answers" => "answers".into(),
        "contributes_to" => "contributes to".into(),
        "confirms_selection" => "confirms the choice made in".into(),
        other => other.replace('_', " "),
    }
}

/// The relation names `relations[]` may declare, plus `none`: structural
/// links change memberships and go through kmp_relabel, and across abouts only
/// the equivalences may be declared. `contradicts`, `supersedes`,
/// `updates_state` and `corrects` compete here, so a later status is told
/// apart from a clash by the same choice.
pub(crate) fn relation_options(crosses_abouts: bool) -> Vec<String> {
    KnownMemoryRelationType::writer_relation_types()
        .iter()
        .filter(|relation| {
            MemoryRelationType::new(relation.as_str())
                .is_ok_and(|relation_type| !relation_type.is_structural())
        })
        .filter(|relation| !crosses_abouts || relation.may_cross_abouts())
        .map(|relation| relation.as_str().to_string())
        .chain(std::iter::once(NONE.to_string()))
        .collect()
}

/// A fact as the judge reads it: its date first when known, then its text.
pub(crate) fn text_of(material: &CurateMaterial, reference: &str) -> String {
    material
        .fact(reference)
        .map(|fact| match &fact.occurred {
            Some(date) => format!("({date}) {}", fact.text),
            None => fact.text.clone(),
        })
        .unwrap_or_default()
}

/// Whether each pair Jev's partner round found holds (`c<n>`), asked as a
/// yes/no of its own over the two texts: the confirming filter of a review
/// without `focus` ([`PartnerFilter::Confirm`]). Notes written from one
/// template about different people or things are named as not holding.
///
/// [`PartnerFilter::Confirm`]: crate::curate::domain::partner_filter::PartnerFilter::Confirm
pub(crate) fn confirm_request(
    material: &CurateMaterial,
    pairs: &[CandidatePair],
) -> JudgementRequest {
    let mut shown = serde_json::Map::new();
    let mut questions = BTreeMap::new();
    for (n, pair) in pairs.iter().enumerate() {
        shown.insert(
            format!("c{n}"),
            json!({
                "a": excerpt(&text_of(material, &pair.from), CONFIRM_CHARS),
                "b": excerpt(&text_of(material, &pair.to), CONFIRM_CHARS),
            }),
        );
        questions.insert(
            format!("c{n}"),
            JudgementQuestion::Noul {
                instructions: json!(format!(
                    "Are `pairs.c{n}.a` and `pairs.c{n}.b` about the same event, thing or \
                     decision, so that one causes, explains, supports, contradicts, answers \
                     or repeats the other? Two notes that follow the same template about \
                     different people, teams or things are not."
                )),
            },
        );
    }
    JudgementRequest {
        state: json!({ "pairs": shown }),
        questions,
    }
}

/// For each pair, its best relation type (`t<n>`). A contradiction is one of
/// the options, so it has to beat `supersedes`, `updates_state` and
/// `corrects` to be proposed.
pub(crate) fn pair_request(material: &CurateMaterial, pairs: &[CandidatePair]) -> JudgementRequest {
    let mut questions = BTreeMap::new();
    for (n, pair) in pairs.iter().enumerate() {
        questions.insert(
            format!("t{n}"),
            JudgementQuestion::Choice {
                instructions: json!({
                    "from": excerpt(&text_of(material, &pair.from), SENT_CHARS),
                    "to": excerpt(&text_of(material, &pair.to), SENT_CHARS),
                    "question": "Which relation does `from` have to `to`? A later status or a newer version of the same thing is not a contradiction. Answer none when no relation holds.",
                }),
                options: relation_options(pair.crosses_abouts),
            },
        );
    }
    JudgementRequest {
        state: json!("Two memories of one knowledge base, judged pair by pair."),
        questions,
    }
}

/// For each declared link: does its reason hold (`s<n>`), and which type
/// fits best (`b<n>`).
pub(crate) fn suspect_request(material: &CurateMaterial) -> JudgementRequest {
    let mut questions = BTreeMap::new();
    for (n, link) in material.declared.iter().enumerate() {
        let crosses = material.fact(&link.from).map(|f| &f.about)
            != material.fact(&link.to).map(|f| &f.about);
        let base = json!({
            "from": excerpt(&text_of(material, &link.from), SENT_CHARS),
            "to": excerpt(&text_of(material, &link.to), SENT_CHARS),
            "relation": link.rel,
            "why": link.why,
            "evidence": link.evidence,
        });
        let mut support = base.clone();
        support["question"] =
            json!("Do `why` and `evidence` show that `from` has the relation `relation` to `to`?");
        questions.insert(
            format!("s{n}"),
            JudgementQuestion::Noul {
                instructions: support,
            },
        );
        let mut best = base;
        best["question"] =
            json!("Which relation does `from` have to `to`? Answer none when no relation holds.");
        questions.insert(
            format!("b{n}"),
            JudgementQuestion::Choice {
                instructions: best,
                options: relation_options(crosses),
            },
        );
    }
    JudgementRequest {
        state: json!(
            "Declared relations between memories of one knowledge base, audited one by one."
        ),
        questions,
    }
}

/// The pre-write check: for each item the agent is about to write, does its
/// own why and evidence hold (`s<n>`), and which type fits best (`b<n>`).
pub(crate) fn precheck_request(
    material: &CurateMaterial,
    items: &[PreparedRelation],
) -> JudgementRequest {
    let mut questions = BTreeMap::new();
    for (n, item) in items.iter().enumerate() {
        let crosses = material.fact(&item.from).map(|f| &f.about)
            != material.fact(&item.to).map(|f| &f.about);
        let base = json!({
            "from": excerpt(&text_of(material, &item.from), SENT_CHARS),
            "to": excerpt(&text_of(material, &item.to), SENT_CHARS),
            "relation": item.rel,
            "why": item.why,
            "evidence": item.evidence,
        });
        let mut support = base.clone();
        support["question"] =
            json!("Do `why` and `evidence` show that `from` has the relation `relation` to `to`?");
        questions.insert(
            format!("s{n}"),
            JudgementQuestion::Noul {
                instructions: support,
            },
        );
        if let Some(direction) = direction_question(
            &text_of(material, &item.from),
            &text_of(material, &item.to),
            &item.rel,
        ) {
            questions.insert(format!("d{n}"), direction);
        }
        let mut best = base;
        best["question"] = json!(
            "Which relation does `from` have to `to`? A later status or a newer version of the same thing is not a contradiction. Answer none when no relation holds."
        );
        questions.insert(
            format!("b{n}"),
            JudgementQuestion::Choice {
                instructions: best,
                options: relation_options(crosses),
            },
        );
    }
    JudgementRequest {
        state: json!(
            "Relations an agent is about to declare between memories of one knowledge base, checked one by one."
        ),
        questions,
    }
}

/// Facts a path search may walk through, at most, once the judge has kept
/// what lies on the way. A choice offers at most 255 options.
pub(crate) const PATH_FACTS: usize = 120;
const PATH_CHARS: usize = 300;

/// For every fact but the ends (`w<n>`): does it lie on the way from the
/// start to the goal, or, with no goal, in what followed from the start?
pub(crate) fn on_the_way_request(
    material: &CurateMaterial,
    start: &str,
    goal: Option<&str>,
    followed: &[String],
    only: Option<&std::collections::BTreeSet<String>>,
) -> (JudgementRequest, Vec<String>) {
    let refs = material
        .facts
        .iter()
        .map(|fact| fact.reference.clone())
        .filter(|reference| {
            reference != start
                && Some(reference.as_str()) != goal
                && !followed.contains(reference)
                && only.is_none_or(|only| only.contains(reference))
        })
        .collect::<Vec<_>>();
    let mut state = json!({"start": excerpt(&text_of(material, start), SENT_CHARS)});
    let question = match goal {
        Some(goal) => {
            state["goal"] = json!(excerpt(&text_of(material, goal), SENT_CHARS));
            "Is `passage` a step in what connects `start` to `goal`?"
        }
        None if followed.is_empty() => "Is `passage` part of what followed from `start`?",
        None => {
            state["followed"] = json!(
                followed
                    .iter()
                    .map(|reference| excerpt(&text_of(material, reference), PATH_CHARS))
                    .collect::<Vec<_>>()
            );
            "Did `passage` happen because of, or in response to, something in `followed`?"
        }
    };
    let questions = refs
        .iter()
        .enumerate()
        .map(|(n, reference)| {
            (
                format!("w{n}"),
                JudgementQuestion::Noul {
                    instructions: json!({
                        "passage": excerpt(&text_of(material, reference), PATH_CHARS),
                        "question": question,
                    }),
                },
            )
        })
        .collect();
    (JudgementRequest { state, questions }, refs)
}

/// For each fact, which other fact is its direct consequence (`n<k>`) and
/// which its direct cause (`c<k>`): a chain needs both directions, and a
/// fact can lead to more than one thing.
/// The facts are the state, dated, keyed `f<k>`.
///
/// With `neighbours` (a goal search's corridor, DESIGN L7), each fact's two
/// choices offer only the facts named for it instead of every other fact.
pub(crate) fn next_step_request(
    material: &CurateMaterial,
    refs: &[String],
    neighbours: Option<&BTreeMap<String, Vec<String>>>,
) -> (JudgementRequest, BTreeMap<String, String>) {
    let keys = refs
        .iter()
        .enumerate()
        .map(|(n, reference)| (format!("f{n}"), reference.clone()))
        .collect::<BTreeMap<_, _>>();
    let state = json!({ "facts": keys
        .iter()
        .map(|(key, reference)| (key.clone(), json!(excerpt(&text_of(material, reference), PATH_CHARS))))
        .collect::<serde_json::Map<_, _>>() });
    let key_of = keys
        .iter()
        .map(|(key, reference)| (reference.as_str(), key.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut questions = BTreeMap::new();
    for (own, reference) in &keys {
        let offered = |key: &String| {
            neighbours.is_none_or(|neighbours| {
                neighbours.get(reference).is_some_and(|options| {
                    options
                        .iter()
                        .any(|other| key_of.get(other.as_str()) == Some(&key.as_str()))
                })
            })
        };
        let options = keys
            .keys()
            .filter(|key| *key != own && offered(key))
            .cloned()
            .chain(std::iter::once(NONE.to_string()))
            .collect::<Vec<_>>();
        questions.insert(
            format!("n{}", &own[1..]),
            JudgementQuestion::Choice {
                instructions: json!(format!(
                    "Which fact in `facts` is a direct consequence of `facts.{own}`: \
                     caused by it, decided because of it, or the next state of the same \
                     thing? Answer none when none is."
                )),
                options: options.clone(),
            },
        );
        questions.insert(
            format!("c{}", &own[1..]),
            JudgementQuestion::Choice {
                instructions: json!(format!(
                    "Which fact in `facts` is a direct cause of `facts.{own}`: it led to \
                     it, it is why it was decided, or it is the earlier state of the same \
                     thing? Answer none when none is."
                )),
                options,
            },
        );
    }
    (JudgementRequest { state, questions }, keys)
}
