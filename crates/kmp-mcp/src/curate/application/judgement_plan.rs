use std::collections::BTreeMap;

use kmp_domain::{KnownMemoryRelationType, MemoryRelationType};
use serde_json::{Value, json};

use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::prepared_relation::PreparedRelation;
use crate::curate::application::relation_wording::{
    CONFIRM_FALSE, CONFIRM_TRUE, DIRECTION_Q, ONWAY_FOLLOWED, ONWAY_FROM, ONWAY_GOAL,
    RELATED_FALSE, RELATED_Q, RELATED_TRUE, SUPPORT_Q, TYPE_Q, confirm_question, either_way,
    relation_means, relation_verb,
};
use crate::curate::domain::candidate_pair::CandidatePair;
use crate::curate::domain::curate_thresholds::SYMMETRIC;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::noul_criteria::NoulCriteria;

const SENT_CHARS: usize = 2_000;
/// Chars of each text a confirming question reads, as the partner round
/// read them.
const CONFIRM_CHARS: usize = 400;

pub(crate) fn excerpt(text: &str, chars: usize) -> String {
    text.chars().take(chars).collect()
}

/// A yes/no question with what its yes and no mean.
pub(crate) fn criteria_noul(
    instructions: Value,
    when_true: &str,
    when_false: &str,
) -> JudgementQuestion {
    JudgementQuestion::CriteriaNoul {
        instructions,
        criteria: NoulCriteria::new(when_true, when_false),
    }
}

/// `base` with its `question` set.
fn asking(base: &Value, question: &str) -> Value {
    let mut asked = base.clone();
    asked["question"] = json!(question);
    asked
}

/// Which way an item about to be written runs, asked over the two dated
/// texts alone, as a choice between two described options (`from_<rel>_to`,
/// `to_<rel>_from`) beside its companion (does either hold?). The author's
/// why is left out on purpose: measured on a reversed declaration, a wrong
/// why moved the judge from 0.01 to 0.86 towards the declared direction.
/// Asked only before a write: auditing stored declarations with it flagged
/// nothing more in three recorded samples (jev-evaluation.md), so the audit
/// does not pay for it.
fn direction_questions(
    texts: &Value,
    relation: &str,
) -> Option<(JudgementQuestion, JudgementQuestion)> {
    if SYMMETRIC.contains(&relation) {
        return None;
    }
    let verb = relation_verb(relation);
    let choice = JudgementQuestion::described_choice(
        asking(texts, DIRECTION_Q),
        vec![
            (forward_option(relation), format!("`from` {verb} `to`.")),
            (backward_option(relation), format!("`to` {verb} `from`.")),
        ],
    );
    let (question, when_true, when_false) = either_way(&verb);
    Some((
        choice,
        criteria_noul(asking(texts, &question), &when_true, &when_false),
    ))
}

/// The option of a direction choice that reads `relation` as declared.
pub(crate) fn forward_option(relation: &str) -> String {
    format!("from_{relation}_to")
}

/// The option of a direction choice that reads `relation` reversed.
pub(crate) fn backward_option(relation: &str) -> String {
    format!("to_{relation}_from")
}

/// Which of two facts was recorded first, as a sentence naming them by the
/// fields that hold them; `None` when either moment is unknown. Jev reads
/// dates as text, so the order is computed here and stated as a fact.
pub(crate) fn order_of(
    material: &CurateMaterial,
    (first, first_name): (&str, &str),
    (second, second_name): (&str, &str),
) -> Option<String> {
    let at = |reference: &str| material.fact(reference).and_then(|fact| fact.occurred_at);
    let (a, b) = (at(first)?, at(second)?);
    Some(match a.cmp(&b) {
        std::cmp::Ordering::Equal => {
            format!("`{first_name}` and `{second_name}` were recorded at the same time.")
        }
        std::cmp::Ordering::Greater => {
            format!("`{first_name}` was recorded after `{second_name}`.")
        }
        std::cmp::Ordering::Less => format!("`{first_name}` was recorded before `{second_name}`."),
    })
}

/// `{from, to}` as the judge reads a pair, dated, with the order they were
/// recorded in when it is known.
fn pair_texts(material: &CurateMaterial, from: &str, to: &str) -> Value {
    let mut texts = json!({
        "from": excerpt(&text_of(material, from), SENT_CHARS),
        "to": excerpt(&text_of(material, to), SENT_CHARS),
    });
    if let Some(order) = order_of(material, (from, "from"), (to, "to")) {
        texts["order"] = json!(order);
    }
    texts
}

/// A typing choice over `options`, each described.
fn type_question(texts: &Value, options: Vec<String>) -> JudgementQuestion {
    JudgementQuestion::described_choice(
        asking(texts, TYPE_Q),
        options
            .into_iter()
            .map(|option| {
                let means = relation_means(&option).unwrap_or_default().to_string();
                (option, means)
            })
            .collect(),
    )
}

/// Whether a relation's own why and evidence hold, with what the relation
/// means.
fn support_question(texts: &Value, rel: &str, why: &str, evidence: &str) -> JudgementQuestion {
    let mut asked = asking(texts, SUPPORT_Q);
    asked["relation"] = json!(rel);
    if let Some(means) = relation_means(rel) {
        asked["relation_means"] = json!(means);
    }
    asked["why"] = json!(why);
    asked["evidence"] = json!(evidence);
    JudgementQuestion::Noul {
        instructions: asked,
    }
}

/// The relation names `relations[]` may declare: structural links change
/// memberships and go through kmp_relabel, and across abouts only the
/// equivalences may be declared. `contradicts`, `supersedes`,
/// `updates_state` and `corrects` compete here, so a later status is told
/// apart from a clash by the same choice. No `none`: whether any relation
/// holds is a companion question of its own.
pub(crate) fn relation_options(crosses_abouts: bool) -> Vec<String> {
    KnownMemoryRelationType::writer_relation_types()
        .iter()
        .filter(|relation| {
            MemoryRelationType::new(relation.as_str())
                .is_ok_and(|relation_type| !relation_type.is_structural())
        })
        .filter(|relation| !crosses_abouts || relation.may_cross_abouts())
        .map(|relation| relation.as_str().to_string())
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
        let key = format!("c{n}");
        shown.insert(
            key.clone(),
            json!({
                "a": excerpt(&text_of(material, &pair.from), CONFIRM_CHARS),
                "b": excerpt(&text_of(material, &pair.to), CONFIRM_CHARS),
            }),
        );
        questions.insert(
            key.clone(),
            criteria_noul(json!(confirm_question(&key)), CONFIRM_TRUE, CONFIRM_FALSE),
        );
    }
    JudgementRequest {
        state: json!({ "pairs": shown }),
        questions,
    }
}

/// For each pair, its best relation type (`t<n>`, every option described)
/// and its companion (`r<n>`): does any direct relation hold? A
/// contradiction is one of the options, so it has to beat `supersedes`,
/// `updates_state` and `corrects` to be proposed. Nothing is shared, so the
/// state is empty.
pub(crate) fn pair_request(material: &CurateMaterial, pairs: &[CandidatePair]) -> JudgementRequest {
    let mut questions = BTreeMap::new();
    for (n, pair) in pairs.iter().enumerate() {
        let texts = pair_texts(material, &pair.from, &pair.to);
        questions.insert(
            format!("t{n}"),
            type_question(&texts, relation_options(pair.crosses_abouts)),
        );
        questions.insert(
            format!("r{n}"),
            criteria_noul(asking(&texts, RELATED_Q), RELATED_TRUE, RELATED_FALSE),
        );
    }
    JudgementRequest {
        state: json!({}),
        questions,
    }
}

/// For each declared link: does its reason hold (`s<n>`), and which type
/// fits best (`b<n>`), read from the texts alone.
pub(crate) fn suspect_request(material: &CurateMaterial) -> JudgementRequest {
    let mut questions = BTreeMap::new();
    for (n, link) in material.declared.iter().enumerate() {
        let crosses = material.fact(&link.from).map(|f| &f.about)
            != material.fact(&link.to).map(|f| &f.about);
        let texts = pair_texts(material, &link.from, &link.to);
        questions.insert(
            format!("s{n}"),
            support_question(&texts, &link.rel, &link.why, &link.evidence),
        );
        questions.insert(
            format!("b{n}"),
            type_question(&texts, relation_options(crosses)),
        );
    }
    JudgementRequest {
        state: json!({}),
        questions,
    }
}

/// The pre-write check: for each item the agent is about to write, does its
/// own why and evidence hold (`s<n>`), which way it runs (`d<n>`, with its
/// companion `e<n>`), and which type fits best (`b<n>`).
pub(crate) fn precheck_request(
    material: &CurateMaterial,
    items: &[PreparedRelation],
) -> JudgementRequest {
    let mut questions = BTreeMap::new();
    for (n, item) in items.iter().enumerate() {
        let crosses = material.fact(&item.from).map(|f| &f.about)
            != material.fact(&item.to).map(|f| &f.about);
        let texts = pair_texts(material, &item.from, &item.to);
        questions.insert(
            format!("s{n}"),
            support_question(&texts, &item.rel, &item.why, &item.evidence),
        );
        if let Some((direction, either)) = direction_questions(&texts, &item.rel) {
            questions.insert(format!("d{n}"), direction);
            questions.insert(format!("e{n}"), either);
        }
        questions.insert(
            format!("b{n}"),
            type_question(&texts, relation_options(crosses)),
        );
    }
    JudgementRequest {
        state: json!({}),
        questions,
    }
}

/// Facts a path search may walk through, at most, once the judge has kept
/// what lies on the way. A choice offers at most 255 options.
pub(crate) const PATH_FACTS: usize = 120;
pub(crate) const PATH_CHARS: usize = 300;

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
    let (question, when_true, when_false) = match goal {
        Some(goal) => {
            state["goal"] = json!(excerpt(&text_of(material, goal), SENT_CHARS));
            ONWAY_GOAL
        }
        None if followed.is_empty() => ONWAY_FROM,
        None => {
            state["followed"] = json!(
                followed
                    .iter()
                    .map(|reference| excerpt(&text_of(material, reference), PATH_CHARS))
                    .collect::<Vec<_>>()
            );
            ONWAY_FOLLOWED
        }
    };
    let questions = refs
        .iter()
        .enumerate()
        .map(|(n, reference)| {
            (
                format!("w{n}"),
                criteria_noul(
                    json!({
                        "passage": excerpt(&text_of(material, reference), PATH_CHARS),
                        "question": question,
                    }),
                    when_true,
                    when_false,
                ),
            )
        })
        .collect();
    (JudgementRequest { state, questions }, refs)
}
