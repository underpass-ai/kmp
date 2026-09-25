use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::jev_usage::JevUsage;
use crate::curate::application::judgement_plan::{PATH_FACTS, on_the_way_request};
use crate::curate::domain::curate_thresholds::PARTNER_AT;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::ports::judgement_model::JudgementModel;

/// Further rounds a goal-less search asks, each from what the chain already
/// holds: a consequence of a consequence need not name the start.
const EXPAND_ROUNDS: usize = 2;

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
        let (request, refs) = on_the_way_request(material, from, to, &kept);
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
        };
        CurateMaterial {
            facts: vec![fact("a"), fact("b"), fact("c")],
            declared: Vec::new(),
            pairs: Vec::new(),
            selection: "fp".into(),
        }
    }

    #[test]
    fn a_later_round_asks_through_the_facts_already_kept() {
        let material = material();
        let (first, refs) = on_the_way_request(&material, "a", None, &[]);
        assert_eq!(refs, vec!["b".to_string(), "c".to_string()]);
        assert!(first.state.get("followed").is_none());
        let (later, refs) = on_the_way_request(&material, "a", None, &["b".to_string()]);
        assert_eq!(
            refs,
            vec!["c".to_string()],
            "a kept fact is not asked again"
        );
        assert_eq!(later.state["followed"].as_array().map(Vec::len), Some(1));
        let (goal, _) = on_the_way_request(&material, "a", Some("c"), &["b".to_string()]);
        assert!(
            goal.state.get("followed").is_none(),
            "a search with a goal asks once"
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
