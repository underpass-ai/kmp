use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::jev_usage::JevUsage;
use crate::curate::application::judgement_plan::{PATH_FACTS, on_the_way_request};
use crate::curate::domain::curate_thresholds::PARTNER_AT;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::ports::judgement_model::JudgementModel;

/// Further rounds a goal-less search asks, each from what the chain already
/// holds: a consequence of a consequence need not name the start.
const EXPAND_ROUNDS: usize = 2;
/// How far over kernel pairs and declared relations a candidate may lie from
/// the facts the chain already holds.
const NEAR_HOPS: usize = 2;

/// Facts within `NEAR_HOPS` of `seeds` over kernel pairs and declared
/// relations: the only ones worth asking about when the graph links them.
/// None when the graph links nothing near, so the judge reads the whole
/// selection as before.
fn near(material: &CurateMaterial, seeds: &[&str]) -> Option<std::collections::BTreeSet<String>> {
    let edges = material
        .pairs
        .iter()
        .map(|pair| (pair.from.as_str(), pair.to.as_str()))
        .chain(
            material
                .declared
                .iter()
                .map(|link| (link.from.as_str(), link.to.as_str())),
        )
        .collect::<Vec<_>>();
    let mut reached = seeds
        .iter()
        .map(|seed| (*seed).to_string())
        .collect::<std::collections::BTreeSet<_>>();
    let mut frontier = reached.clone();
    for _ in 0..NEAR_HOPS {
        let mut next = std::collections::BTreeSet::new();
        for (left, right) in &edges {
            for (here, there) in [(left, right), (right, left)] {
                if frontier.contains(*here) && !reached.contains(*there) {
                    next.insert((*there).to_string());
                }
            }
        }
        if next.is_empty() {
            break;
        }
        reached.extend(next.iter().cloned());
        frontier = next;
    }
    (reached.len() > seeds.len()).then_some(reached)
}

/// The facts the judge places on the way from `from` (to `to`), strongest
/// first. Without a goal, each round asks again what followed through the
/// facts already kept, until a round adds nothing.
pub(crate) async fn facts_on_the_way(
    model: &dyn JudgementModel,
    material: &CurateMaterial,
    from: &str,
    to: Option<&str>,
    usage: &mut JevUsage,
) -> Result<Vec<String>, String> {
    let mut kept = Vec::<String>::new();
    let rounds = if to.is_some() { 1 } else { 1 + EXPAND_ROUNDS };
    for _ in 0..rounds {
        let seeds = std::iter::once(from)
            .chain(to)
            .chain(kept.iter().map(String::as_str))
            .collect::<Vec<_>>();
        // With a goal, the facts the graph links near either end are the
        // ones worth asking about: on the judged corpus that kept every path
        // found at 43% fewer tokens. Without one it lost the chain whose next
        // step the graph does not link yet, so the judge reads everything.
        let only = to.and_then(|_| near(material, &seeds));
        let (request, refs) = on_the_way_request(material, from, to, &kept, only.as_ref());
        if refs.is_empty() {
            break;
        }
        let response = model.evaluate(&request).await?;
        usage.requests += response.requests;
        usage.input_tokens += response.input_tokens;
        let mut added = refs
            .iter()
            .enumerate()
            .filter_map(
                |(n, reference)| match response.answers.get(&format!("w{n}")) {
                    Some(JudgementAnswer::Noul { yes }) if *yes >= PARTNER_AT => {
                        Some((*yes, reference.clone()))
                    }
                    _ => None,
                },
            )
            .collect::<Vec<_>>();
        added.sort_by(|left, right| {
            right
                .0
                .total_cmp(&left.0)
                .then_with(|| left.1.cmp(&right.1))
        });
        if added.is_empty() {
            break;
        }
        kept.extend(added.into_iter().map(|(_, reference)| reference));
        if kept.len() >= PATH_FACTS - 2 {
            break;
        }
    }
    kept.truncate(PATH_FACTS - 2);
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curate::domain::curate_fact::CurateFact;

    fn material() -> CurateMaterial {
        let fact = |reference: &str| CurateFact {
            reference: reference.into(),
            about: "a".into(),
            text: format!("text {reference}"),
            occurred: None,
            labels: Vec::new(),
        };
        CurateMaterial {
            facts: vec![fact("a"), fact("b"), fact("c")],
            declared: Vec::new(),
            pairs: Vec::new(),
            selection: "fp".into(),
            past: Vec::new(),
        }
    }

    #[test]
    fn a_later_round_asks_through_the_facts_already_kept() {
        let material = material();
        let (first, refs) = on_the_way_request(&material, "a", None, &[], None);
        assert_eq!(refs, vec!["b".to_string(), "c".to_string()]);
        assert!(first.state.get("followed").is_none());
        let (later, refs) = on_the_way_request(&material, "a", None, &["b".to_string()], None);
        assert_eq!(
            refs,
            vec!["c".to_string()],
            "a kept fact is not asked again"
        );
        assert_eq!(later.state["followed"].as_array().map(Vec::len), Some(1));
        let (goal, _) = on_the_way_request(&material, "a", Some("c"), &["b".to_string()], None);
        assert!(
            goal.state.get("followed").is_none(),
            "a search with a goal asks once"
        );
    }

    #[test]
    fn near_reaches_two_hops_over_pairs_and_declarations_or_nothing() {
        use crate::curate::domain::{
            candidate_pair::CandidatePair, declared_link::DeclaredLink, pair_origin::PairOrigin,
        };
        let mut linked = material();
        assert_eq!(near(&linked, &["a"]), None, "nothing links a");
        linked.pairs.push(CandidatePair {
            from: "a".into(),
            to: "b".into(),
            origin: PairOrigin::Jev,
            crosses_abouts: false,
        });
        linked.declared.push(DeclaredLink {
            from: "c".into(),
            to: "b".into(),
            rel: "supports".into(),
            why: "w".into(),
            evidence: "e".into(),
        });
        let reached = near(&linked, &["a"]).expect("linked");
        assert!(
            reached.contains("b") && reached.contains("c"),
            "{reached:?}"
        );
    }

    #[tokio::test]
    async fn rounds_stop_when_nothing_is_left_to_ask() {
        use crate::curate::application::use_cases::scripted_judgement::Scripted;
        use std::sync::Mutex;
        let model = Scripted {
            noul: 0.9,
            choice: "none",
            confidence: 0.9,
            calls: Mutex::new(0),
        };
        let mut usage = JevUsage {
            model: "jev-test".into(),
            requests: 0,
            input_tokens: 0,
        };
        let kept = facts_on_the_way(&model, &material(), "a", None, &mut usage)
            .await
            .expect("judged");
        assert_eq!(kept, vec!["b".to_string(), "c".to_string()]);
        assert_eq!(*model.calls.lock().expect("calls"), 1);
    }
}
