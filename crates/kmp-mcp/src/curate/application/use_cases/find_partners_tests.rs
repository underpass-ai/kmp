use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use super::*;
use crate::curate::domain::curate_fact::CurateFact;
use crate::serving::judgement_question::JudgementQuestion;

fn fact(n: usize, text: &str) -> CurateFact {
    CurateFact {
        reference: format!("r{n}"),
        about: "a".into(),
        kind: String::new(),
        text: text.into(),
        occurred: None,
        occurred_at: None,
        labels: Vec::new(),
    }
}

/// `size` facts; the facts in `paired` are touched by a kernel pair, so
/// every other fact is an orphan.
fn material(texts: &[String], paired: &[usize]) -> CurateMaterial {
    CurateMaterial {
        facts: texts
            .iter()
            .enumerate()
            .map(|(n, text)| fact(n, text))
            .collect(),
        declared: Vec::new(),
        pairs: paired
            .chunks(2)
            .map(|pair| CandidatePair {
                from: format!("r{}", pair[0]),
                to: format!("r{}", pair[1]),
                origin: PairOrigin::Jev,
                crosses_abouts: false,
            })
            .collect(),
        selection: "fp".into(),
        past: Vec::new(),
    }
}

/// Answers each choice with the first key it likes that is offered (`likes`
/// maps an orphan's key to the keys it likes) at probability 0.9, and its
/// companion (`<key>.any`) yes when one is offered, else no; each other noul
/// with yes when its key is in `confirmed`, else 0.1. Records every request.
struct Judge {
    likes: BTreeMap<String, Vec<String>>,
    confirmed: BTreeSet<&'static str>,
    asked: Mutex<Vec<JudgementRequest>>,
}

/// The key of fact `n` of [`texts`].
fn k(n: usize) -> String {
    format!("note_{n}")
}

impl Judge {
    fn new(likes: &[(usize, &[usize])]) -> Self {
        Self::keyed(likes, k)
    }

    /// `likes` by fact index, with each fact's key from `key`.
    fn keyed(likes: &[(usize, &[usize])], key: impl Fn(usize) -> String) -> Self {
        Self {
            likes: likes
                .iter()
                .map(|(own, liked)| (key(*own), liked.iter().map(|n| key(*n)).collect()))
                .collect(),
            confirmed: BTreeSet::new(),
            asked: Mutex::new(Vec::new()),
        }
    }

    fn asked(&self) -> Vec<JudgementRequest> {
        self.asked.lock().expect("asked").clone()
    }

    fn liked(&self, own: &str, request: &JudgementRequest) -> Option<String> {
        let JudgementQuestion::Choice { options, .. } = request.questions.get(own)? else {
            return None;
        };
        self.likes
            .get(own)?
            .iter()
            .find(|like| options.contains(like))
            .cloned()
    }

    fn answer(&self, request: &JudgementRequest) -> JudgementResponse {
        let mut response = JudgementResponse::empty("jev-test");
        for (key, question) in &request.questions {
            let answer = match question {
                JudgementQuestion::Choice { options, .. } => {
                    let choice = self
                        .liked(key, request)
                        .unwrap_or_else(|| options[0].clone());
                    JudgementAnswer::Choice {
                        probabilities: BTreeMap::from([(choice.clone(), 0.9)]),
                        choice,
                        confidence: 0.0,
                    }
                }
                JudgementQuestion::CriteriaNoul { .. } | JudgementQuestion::Noul { .. } => {
                    let yes = match key.strip_suffix(".any") {
                        Some(own) => self.liked(own, request).is_some(),
                        None => self.confirmed.contains(key.as_str()),
                    };
                    JudgementAnswer::Noul {
                        yes: if yes { 0.9 } else { 0.1 },
                    }
                }
                JudgementQuestion::Score { .. } => panic!("partners ask no score"),
            };
            response.answers.insert(key.clone(), answer);
        }
        response.requests = 1;
        self.asked.lock().expect("asked").push(request.clone());
        response
    }
}

impl JudgementModel for Judge {
    fn model(&self) -> &str {
        "jev-test"
    }

    fn evaluate<'a>(
        &'a self,
        request: &'a JudgementRequest,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<JudgementResponse, String>> + Send + 'a>,
    > {
        let response = self.answer(request);
        Box::pin(async move { Ok(response) })
    }
}

async fn run(
    material: &CurateMaterial,
    cap: &str,
    filter: PartnerFilter,
    judge: &Judge,
) -> (Vec<(String, String)>, Vec<String>) {
    let mut usage = JevUsage::new("jev-test");
    let mut failures = Vec::new();
    let (pairs, warnings) = FindPartners {
        cap: PartnerCap::named(cap).expect("cap"),
        filter,
    }
    .run(judge, material, &mut usage, &mut failures)
    .await;
    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(usage.requests, judge.asked().len());
    (
        pairs.into_iter().map(|pair| (pair.from, pair.to)).collect(),
        warnings,
    )
}

fn texts(size: usize) -> Vec<String> {
    (0..size).map(|n| format!("note {n}")).collect()
}

#[tokio::test]
async fn an_about_within_one_choice_asks_once_as_it_always_did() {
    let material = material(&texts(40), &[]);
    let judge = Judge::new(&[(3, &[9]), (12, &[1])]);
    let (pairs, warnings) = run(&material, "120", PartnerFilter::Off, &judge).await;
    assert_eq!(judge.asked().len(), 1);
    assert_eq!(
        judge.asked()[0].questions.len(),
        60,
        "at most 30 orphans, each a choice and its companion"
    );
    assert!(warnings.is_empty());
    assert_eq!(
        pairs,
        vec![
            ("r3".to_string(), "r9".to_string()),
            ("r12".to_string(), "r1".to_string())
        ],
        "in the about's order of the orphans"
    );
}

#[tokio::test]
async fn a_large_about_is_read_in_windows_then_one_final_choice_between_winners() {
    // 400 facts: two windows of 200. Orphans are f0..f2 (every other fact
    // is paired by the kernel).
    let paired = (3..400).collect::<Vec<_>>();
    let material = material(&texts(400), &paired[..396]);
    let orphans = material
        .orphans()
        .iter()
        .map(|fact| fact.reference.clone())
        .collect::<Vec<_>>();
    assert_eq!(orphans, ["r0", "r1", "r2", "r399"]);
    let judge = Judge::new(&[
        // Two winners, one in each window: the final choice takes 300.
        (0, &[300, 10]),
        // One winner: it stands.
        (1, &[250]),
    ]);
    let (pairs, warnings) = run(&material, "512", PartnerFilter::Off, &judge).await;
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(judge.asked().len(), 3, "two windows and one final choice");
    let asked = judge.asked();
    for request in &asked[..2] {
        for question in request.questions.values() {
            if let JudgementQuestion::Choice { options, .. } = question {
                assert!(options.len() <= 239);
            }
        }
    }
    let JudgementQuestion::Choice { options, .. } = &asked[2].questions[&k(0)] else {
        panic!("a choice");
    };
    assert_eq!(options, &[k(10), k(300)]);
    assert_eq!(asked[2].questions.len(), 2, "only fact 0 is contested");
    assert_eq!(
        pairs,
        vec![
            ("r0".to_string(), "r300".to_string()),
            ("r1".to_string(), "r250".to_string())
        ]
    );
}

#[tokio::test]
async fn past_the_cap_the_about_is_named_and_nothing_is_asked() {
    let material = material(&texts(130), &[]);
    let judge = Judge::new(&[]);
    let (pairs, warnings) = run(&material, "120", PartnerFilter::Off, &judge).await;
    assert!(pairs.is_empty());
    assert!(judge.asked().is_empty());
    assert!(warnings[0].contains("130 current facts"), "{warnings:?}");
    assert!(warnings[0].contains("at most 120"), "{warnings:?}");
}

#[tokio::test]
async fn the_rare_term_filter_keeps_only_pairs_that_share_a_rare_term() {
    let mut notes = texts(100);
    notes[1] = "The pricing committee approved a 12% price increase.".into();
    notes[2] = "Churn rose to 4.1% after the price increase.".into();
    notes[3] = "Hugo reviewed the ledger pipeline.".into();
    notes[4] = "Sara reviewed the ledger pipeline.".into();
    for (n, note) in notes.iter_mut().enumerate().skip(5).take(20) {
        *note = format!("Person {n} reviewed the ledger pipeline.");
    }
    let material = material(&notes, &[]);
    let likes: &[(usize, &[usize])] = &[(1, &[2]), (3, &[4])];
    let keys = crate::curate::application::fact_keys::fact_keys(notes.iter().map(String::as_str));
    let judge = || Judge::keyed(likes, |n| keys[n].clone());
    let (kept, _) = run(&material, "120", PartnerFilter::RareTerm, &judge()).await;
    assert_eq!(kept, vec![("r1".to_string(), "r2".to_string())]);
    let (all, _) = run(&material, "120", PartnerFilter::Off, &judge()).await;
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn the_confirming_filter_asks_one_noul_per_pair_and_keeps_the_confirmed() {
    let material = material(&texts(20), &[]);
    let mut judge = Judge::new(&[(1, &[2]), (3, &[4])]);
    judge.confirmed.insert("c1");
    let (kept, _) = run(&material, "120", PartnerFilter::Confirm, &judge).await;
    assert_eq!(judge.asked().len(), 2, "the round and one confirmation");
    let asked = judge.asked();
    let confirming = &asked[1];
    assert_eq!(confirming.questions.len(), 2);
    assert!(
        confirming
            .questions
            .values()
            .all(|question| matches!(question, JudgementQuestion::CriteriaNoul { .. }))
    );
    assert_eq!(confirming.state["pairs"]["c0"]["a"], "note 1");
    assert_eq!(kept, vec![("r3".to_string(), "r4".to_string())]);
}
