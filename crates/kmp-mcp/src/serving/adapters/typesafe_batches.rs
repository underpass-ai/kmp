use std::collections::BTreeMap;

use serde_json::Value;

use super::typesafe_request_body::typesafe_request_body;
use crate::serving::judgement_failure::JudgementFailure;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;

/// Tokens one provider request may carry, the state included (decided
/// 27 Sept 2026: a 104 KB request was refused with `max_tokens_exceeded`
/// under the former 60k budget).
pub(super) const REQUEST_TOKENS: usize = 24_000;
pub(super) const REQUEST_BYTES: usize = 256 * 1024;
/// JSON bytes per provider token, as a fraction `BYTES_PER_TOKEN.0 /
/// BYTES_PER_TOKEN.1`. Measured over every request of the recorded
/// cassettes (27 Sept 2026, 108 requests): median 2.65 bytes a token, worst
/// 1.52 (a three-noul request) and 1.63 (a partner choice of 3,540
/// options). The estimate takes 1.5, below the worst.
const BYTES_PER_TOKEN: (usize, usize) = (3, 2);
/// Room for what a request adds around its state and questions: the pinned
/// model's name and the body's own braces, never more than 96 bytes.
const ENVELOPE_TOKENS: usize = 64;

/// Conservative: the provider tokens of a JSON value at the worst measured
/// density.
pub(super) fn estimated_tokens(value: &Value) -> usize {
    (value.to_string().len() * BYTES_PER_TOKEN.1).div_ceil(BYTES_PER_TOKEN.0)
}

/// How one judgement goes out: the state every request repeats, cut when
/// the whole state and the largest question would not fit one request, and
/// the questions split into requests that fit, in key order.
#[derive(Debug, PartialEq)]
pub(super) struct TypeSafeBatches {
    pub(super) state: Value,
    pub(super) batches: Vec<BTreeMap<String, JudgementQuestion>>,
    /// The longest text the state kept, in chars, when it had to be cut.
    pub(super) cut_to: Option<usize>,
}

impl TypeSafeBatches {
    /// Splits `request` into requests of at most [`REQUEST_TOKENS`]
    /// estimated tokens. When the state and one question cannot share a
    /// request, every text of the state longer than some length is cut to
    /// it, the longest length that fits, so the same request is always cut
    /// the same way. A question that does not fit even beside an emptied
    /// state is refused here: nothing is sent that the provider would
    /// reject.
    pub(super) fn plan(request: &JudgementRequest) -> Result<Self, String> {
        let mut costs = Vec::with_capacity(request.questions.len());
        for (key, question) in &request.questions {
            if let JudgementQuestion::Choice { options, .. } = question
                && !(2..=255).contains(&options.len())
            {
                return Err(format!("choice `{key}` needs between 2 and 255 options"));
            }
            if let JudgementQuestion::Score { levels, .. } = question
                && !(2..=16).contains(&levels.len())
            {
                return Err(format!("score `{key}` needs between 2 and 16 levels"));
            }
            let single = BTreeMap::from([(key.clone(), question.clone())]);
            costs.push(estimated_tokens(&typesafe_request_body(
                "",
                &Value::Null,
                &single,
            )));
        }
        let largest = costs.iter().copied().max().unwrap_or(0);
        let (state, cut_to) = fitted_state(&request.state, largest).ok_or_else(|| {
            let key = request
                .questions
                .keys()
                .zip(&costs)
                .find(|(_, cost)| **cost == largest)
                .map(|(key, _)| key.as_str())
                .unwrap_or_default();
            JudgementFailure::refused(&format!(
                "judgement over budget: question `{key}` does not fit a {}k-token request even with the state's texts cut",
                REQUEST_TOKENS / 1_000
            ))
        })?;
        let state_tokens = estimated_tokens(&state) + ENVELOPE_TOKENS;
        let mut batches = Vec::new();
        let mut current = BTreeMap::new();
        let mut current_tokens = state_tokens;
        for ((key, question), tokens) in request.questions.iter().zip(costs) {
            if !current.is_empty() && current_tokens + tokens > REQUEST_TOKENS {
                batches.push(std::mem::take(&mut current));
                current_tokens = state_tokens;
            }
            current.insert(key.clone(), question.clone());
            current_tokens += tokens;
        }
        if !current.is_empty() {
            batches.push(current);
        }
        Ok(Self {
            state,
            batches,
            cut_to,
        })
    }

    /// The state the provider would read for `request`: the whole state, or
    /// the cut one; the whole state when the request cannot go out at all.
    pub(super) fn wire_state(request: &JudgementRequest) -> Value {
        Self::plan(request).map_or_else(|_| request.state.clone(), |plan| plan.state)
    }
}

/// `state` whole when it fits beside a question of `question_tokens`, else
/// with every string cut to the longest length that fits; None when not
/// even an emptied state does.
fn fitted_state(state: &Value, question_tokens: usize) -> Option<(Value, Option<usize>)> {
    let fits = |value: &Value| {
        estimated_tokens(value) + ENVELOPE_TOKENS + question_tokens <= REQUEST_TOKENS
    };
    if fits(state) {
        return Some((state.clone(), None));
    }
    let (mut low, mut high) = (0, longest_text(state));
    if !fits(&cut(state, low)) {
        return None;
    }
    // The longest cut that fits: `low` always fits, `high` never does.
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if fits(&cut(state, middle)) {
            low = middle;
        } else {
            high = middle;
        }
    }
    Some((cut(state, low), Some(low)))
}

fn longest_text(value: &Value) -> usize {
    match value {
        Value::String(text) => text.chars().count(),
        Value::Array(items) => items.iter().map(longest_text).max().unwrap_or(0),
        Value::Object(fields) => fields.values().map(longest_text).max().unwrap_or(0),
        _ => 0,
    }
}

/// `value` with every string longer than `chars` cut to its first `chars`;
/// keys, numbers and shorter strings unchanged.
fn cut(value: &Value, chars: usize) -> Value {
    match value {
        Value::String(text) if text.chars().count() > chars => {
            Value::String(text.chars().take(chars).collect())
        }
        Value::Array(items) => Value::Array(items.iter().map(|item| cut(item, chars)).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, field)| (key.clone(), cut(field, chars)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn noul(text: &str) -> JudgementQuestion {
        JudgementQuestion::Noul {
            instructions: json!(text),
        }
    }

    fn choice(options: usize) -> JudgementQuestion {
        JudgementQuestion::Choice {
            instructions: json!("Which fact relates to `facts.f0`?"),
            options: (1..options)
                .map(|n| format!("f{n}"))
                .chain(std::iter::once("none".to_string()))
                .collect(),
        }
    }

    fn body_tokens(plan: &TypeSafeBatches, batch: &BTreeMap<String, JudgementQuestion>) -> usize {
        estimated_tokens(&typesafe_request_body("jev-1.13.0", &plan.state, batch))
    }

    #[test]
    fn the_estimate_is_the_worst_measured_density() {
        assert_eq!(estimated_tokens(&json!("")), 2, "two quote bytes");
        assert_eq!(estimated_tokens(&json!("x".repeat(298))), 200);
    }

    #[test]
    fn small_requests_stay_whole_and_large_ones_split_by_budget_in_key_order() {
        let small = JudgementRequest {
            state: json!("s"),
            questions: BTreeMap::from([("a".into(), noul("q")), ("b".into(), noul("q"))]),
        };
        let plan = TypeSafeBatches::plan(&small).expect("fits");
        assert_eq!(plan.batches.len(), 1);
        assert_eq!(plan.state, small.state);
        assert_eq!(plan.cut_to, None);

        let passage = "x".repeat(6_000);
        let large = JudgementRequest {
            state: json!({"facts": {"f0": "y".repeat(3_000)}}),
            questions: (0..20)
                .map(|n| (format!("c{n:02}"), noul(&passage)))
                .collect(),
        };
        let plan = TypeSafeBatches::plan(&large).expect("splits");
        assert!(plan.batches.len() > 1);
        for batch in &plan.batches {
            assert!(
                body_tokens(&plan, batch) <= REQUEST_TOKENS,
                "every request fits the budget, the state included"
            );
        }
        let keys = plan
            .batches
            .iter()
            .flat_map(|batch| batch.keys().cloned())
            .collect::<Vec<_>>();
        assert_eq!(keys, large.questions.keys().cloned().collect::<Vec<_>>());
    }

    #[test]
    fn the_split_is_stable_and_fills_each_request_before_the_next() {
        let request = JudgementRequest {
            state: json!({"facts": {"f0": "a fact"}}),
            questions: (0..90).map(|n| (format!("f{n}"), choice(240))).collect(),
        };
        let first = TypeSafeBatches::plan(&request).expect("fits");
        let second = TypeSafeBatches::plan(&request).expect("fits");
        assert_eq!(first, second, "the same request splits the same way");
        let sizes = first.batches.iter().map(BTreeMap::len).collect::<Vec<_>>();
        assert!(sizes.len() > 1, "{sizes:?}");
        assert!(
            sizes.windows(2).all(|pair| pair[0] >= pair[1]),
            "greedy in key order: only the last request is short: {sizes:?}"
        );
        for batch in &first.batches {
            assert!(body_tokens(&first, batch) <= REQUEST_TOKENS);
        }
    }

    #[test]
    fn a_state_too_large_for_one_question_is_cut_the_same_way_every_time() {
        let facts = (0..400)
            .map(|n| (format!("f{n}"), json!(format!("{n} ").repeat(80))))
            .collect::<serde_json::Map<_, _>>();
        let request = JudgementRequest {
            state: json!({"facts": facts}),
            questions: BTreeMap::from([("f0".into(), choice(240)), ("f1".into(), noul("q"))]),
        };
        assert!(estimated_tokens(&request.state) > REQUEST_TOKENS);
        let plan = TypeSafeBatches::plan(&request).expect("cut to fit");
        let cut_to = plan.cut_to.expect("the state was cut");
        assert!(cut_to > 0 && cut_to < 320, "{cut_to}");
        assert_eq!(plan, TypeSafeBatches::plan(&request).expect("again"));
        assert_eq!(TypeSafeBatches::wire_state(&request), plan.state);
        for batch in &plan.batches {
            assert!(body_tokens(&plan, batch) <= REQUEST_TOKENS);
        }
        // Every fact keeps its key and the start of its text.
        let kept = plan.state["facts"].as_object().expect("facts");
        assert_eq!(kept.len(), 400);
        assert!(kept["f7"].as_str().expect("text").starts_with("7 7 "));
        assert!(
            kept.values()
                .all(|text| text.as_str().expect("text").chars().count() <= cut_to)
        );
        // One more char per text would not have fitted.
        let largest = estimated_tokens(&typesafe_request_body(
            "",
            &Value::Null,
            &BTreeMap::from([("f0".to_string(), choice(240))]),
        ));
        let room = REQUEST_TOKENS - ENVELOPE_TOKENS - largest;
        assert!(estimated_tokens(&cut(&request.state, cut_to)) <= room);
        assert!(estimated_tokens(&cut(&request.state, cut_to + 1)) > room);
    }

    #[test]
    fn a_state_that_fits_is_sent_whole() {
        let request = JudgementRequest {
            state: json!({"facts": {"f0": "short", "f1": "also short"}}),
            questions: BTreeMap::from([("f0".into(), choice(2))]),
        };
        assert_eq!(TypeSafeBatches::wire_state(&request), request.state);
    }

    #[test]
    fn over_budget_questions_and_bad_choices_are_refused() {
        let huge = JudgementRequest {
            state: json!("s"),
            questions: BTreeMap::from([("a".into(), noul(&"x".repeat(40_000)))]),
        };
        let refused = TypeSafeBatches::plan(&huge).expect_err("one question over budget");
        assert!(
            matches!(JudgementFailure::read(&refused), JudgementFailure::Refused(reason) if reason.starts_with("judgement over budget")),
            "{refused}"
        );
        assert!(refused.contains("`a`"), "{refused}");
        for options in [
            vec!["only".to_string()],
            (0..256).map(|n| n.to_string()).collect(),
        ] {
            let bad = JudgementRequest {
                state: json!("s"),
                questions: BTreeMap::from([(
                    "a".into(),
                    JudgementQuestion::Choice {
                        instructions: json!("q"),
                        options,
                    },
                )]),
            };
            assert!(TypeSafeBatches::plan(&bad).is_err());
        }
    }

    #[test]
    fn no_questions_means_no_requests() {
        let empty = JudgementRequest {
            state: json!("s"),
            questions: BTreeMap::new(),
        };
        assert!(
            TypeSafeBatches::plan(&empty)
                .expect("empty")
                .batches
                .is_empty()
        );
    }
}
