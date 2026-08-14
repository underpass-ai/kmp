use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryArenaAnswerScore {
    pub expected_answer_kind: String,
    pub expected_answers: Vec<String>,
    pub hard_success: bool,
    pub candidate_answer_hit: bool,
    pub soft_score: Option<f64>,
    pub soft_score_basis: Option<&'static str>,
}

pub fn memoryarena_task_success_rule(task_type: &str) -> &'static str {
    match task_type {
        "bundled_shopping" | "group_travel_planner" => "all_subtasks_hard_success",
        "progressive_search" | "formal_reasoning_math" | "formal_reasoning_phys" => {
            "final_subtask_hard_success"
        }
        _ => "final_subtask_hard_success",
    }
}

pub fn score_memoryarena_answer(
    task_type: &str,
    expected: &Value,
    candidate_text: Option<&str>,
) -> MemoryArenaAnswerScore {
    let candidate_text = candidate_text.unwrap_or_default();
    match expected {
        Value::Object(object) if object.contains_key("target_asin") => {
            score_shopping_answer(expected, candidate_text)
        }
        Value::Array(_) if task_type == "group_travel_planner" => {
            score_travel_plan_answer(expected, candidate_text)
        }
        Value::Object(_) if task_type == "group_travel_planner" => {
            score_travel_plan_answer(expected, candidate_text)
        }
        _ => score_exact_answer(expected, candidate_text),
    }
}

pub fn memoryarena_answer_candidates_from_value(value: &Value) -> Vec<String> {
    match value {
        Value::String(value) => memoryarena_answer_candidates_from_text(value),
        Value::Array(values) => deduplicate_answers(
            values
                .iter()
                .flat_map(memoryarena_answer_candidates_from_value)
                .collect::<Vec<_>>(),
        ),
        Value::Object(object) => {
            if let Some(value) = object.get("target_asin") {
                let candidates = memoryarena_answer_candidates_from_value(value);
                if !candidates.is_empty() {
                    return candidates;
                }
            }
            for key in ["exact_answer", "exactAnswer", "answer", "target"] {
                if let Some(value) = object.get(key) {
                    let candidates = memoryarena_answer_candidates_from_value(value);
                    if !candidates.is_empty() {
                        return candidates;
                    }
                }
            }
            normalized_fallback_candidates(&value.to_string())
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {
            normalized_fallback_candidates(&value.to_string())
        }
    }
}

pub fn memoryarena_answer_candidates_from_text(value: &str) -> Vec<String> {
    let exact = exact_answer_candidates_from_text(value);
    if !exact.is_empty() {
        return exact;
    }
    normalized_fallback_candidates(value)
}

pub fn memoryarena_answers_match(expected: &str, candidate: &str) -> bool {
    for expected in answer_match_alternatives(expected) {
        for candidate in answer_match_alternatives(candidate) {
            if normalized_answers_match(&expected, &candidate) {
                return true;
            }
        }
    }
    false
}

fn score_shopping_answer(expected: &Value, candidate_text: &str) -> MemoryArenaAnswerScore {
    let target_asin = expected
        .get("target_asin")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    let mut expected_answers = Vec::new();
    push_candidate(&mut expected_answers, target_asin);
    if let Some(attributes) = expected.get("attributes") {
        for attribute in memoryarena_answer_candidates_from_value(attributes) {
            push_candidate(&mut expected_answers, &attribute);
        }
    }
    let hard_success = !target_asin.is_empty() && text_contains_answer(candidate_text, target_asin);
    let attribute_values = expected
        .get("attributes")
        .map(meaningful_json_leaf_values)
        .unwrap_or_default();
    let soft_score = if attribute_values.is_empty() {
        None
    } else {
        let matched = attribute_values
            .iter()
            .filter(|attribute| text_contains_answer(candidate_text, attribute))
            .count();
        Some(matched as f64 / attribute_values.len() as f64)
    };

    MemoryArenaAnswerScore {
        expected_answer_kind: "target_asin".to_string(),
        expected_answers,
        hard_success,
        candidate_answer_hit: hard_success,
        soft_score,
        soft_score_basis: soft_score.map(|_| "shopping_attribute_text_coverage"),
    }
}

fn score_travel_plan_answer(expected: &Value, candidate_text: &str) -> MemoryArenaAnswerScore {
    let expected_answers = meaningful_json_leaf_values(expected);
    let matched = expected_answers
        .iter()
        .filter(|expected| text_contains_answer(candidate_text, expected))
        .count();
    let soft_score = if expected_answers.is_empty() {
        None
    } else {
        Some(matched as f64 / expected_answers.len() as f64)
    };
    let hard_success = !expected_answers.is_empty() && matched == expected_answers.len();

    MemoryArenaAnswerScore {
        expected_answer_kind: "travel_plan_slots".to_string(),
        expected_answers,
        hard_success,
        candidate_answer_hit: hard_success,
        soft_score,
        soft_score_basis: soft_score.map(|_| "travel_expected_slot_text_coverage_proxy"),
    }
}

fn score_exact_answer(expected: &Value, candidate_text: &str) -> MemoryArenaAnswerScore {
    let expected_answers = memoryarena_answer_candidates_from_value(expected);
    let candidate_answers = memoryarena_answer_candidates_from_text(candidate_text);
    let hard_success = expected_answers.iter().any(|expected| {
        candidate_answers
            .iter()
            .any(|candidate| memoryarena_answers_match(expected, candidate))
            || text_contains_answer(candidate_text, expected)
    });

    MemoryArenaAnswerScore {
        expected_answer_kind: expected_answer_kind(expected).to_string(),
        expected_answers,
        hard_success,
        candidate_answer_hit: hard_success,
        soft_score: None,
        soft_score_basis: None,
    }
}

fn expected_answer_kind(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
    }
}

fn exact_answer_candidates_from_text(value: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut use_next_non_empty_line = false;

    for line in value.lines() {
        let normalized_line = normalize_exact_answer_line(line);
        if use_next_non_empty_line {
            if !normalized_line.trim().is_empty() {
                push_candidate(&mut candidates, normalized_line.trim());
                use_next_non_empty_line = false;
            }
            continue;
        }

        let lower = normalized_line.to_ascii_lowercase();
        let Some(label_start) = lower.find("exact answer:") else {
            continue;
        };
        let answer_start = label_start + "exact answer:".len();
        let candidate = normalized_line[answer_start..].trim();
        if candidate.is_empty() {
            use_next_non_empty_line = true;
        } else {
            push_candidate(&mut candidates, candidate);
        }
    }

    candidates
}

fn normalize_exact_answer_line(line: &str) -> String {
    line.trim()
        .trim_start_matches(['-', '*', ' '])
        .replace(['*', '`'], "")
        .trim()
        .to_string()
}

fn meaningful_json_leaf_values(value: &Value) -> Vec<String> {
    let mut values = Vec::new();
    collect_meaningful_json_leaf_values(value, &mut values);
    deduplicate_answers(values)
}

fn collect_meaningful_json_leaf_values(value: &Value, values: &mut Vec<String>) {
    match value {
        Value::String(text) => {
            let text = trim_candidate_answer(text);
            if is_meaningful_leaf_value(&text) {
                values.push(text);
            }
        }
        Value::Number(number) => values.push(number.to_string()),
        Value::Array(items) => {
            for item in items {
                collect_meaningful_json_leaf_values(item, values);
            }
        }
        Value::Object(object) => {
            for value in object.values() {
                collect_meaningful_json_leaf_values(value, values);
            }
        }
        Value::Null | Value::Bool(_) => {}
    }
}

fn is_meaningful_leaf_value(value: &str) -> bool {
    let normalized = value.trim();
    !normalized.is_empty() && normalized != "-" && normalized != "N/A"
}

fn text_contains_answer(text: &str, expected: &str) -> bool {
    if expected.trim().is_empty() {
        return false;
    }
    if memoryarena_answers_match(expected, text) {
        return true;
    }
    let expected = normalize_for_answer_match(expected);
    let candidate = normalize_for_answer_match(text);
    looks_like_identifier(&expected) && candidate.contains(&expected)
}

fn looks_like_identifier(value: &str) -> bool {
    let compact = value.replace(' ', "");
    compact.len() >= 8
        && compact
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        && compact.chars().any(|ch| ch.is_ascii_digit())
}

fn normalized_fallback_candidates(value: &str) -> Vec<String> {
    let candidate = trim_candidate_answer(value);
    if candidate.is_empty() {
        Vec::new()
    } else {
        vec![candidate]
    }
}

fn push_candidate(candidates: &mut Vec<String>, value: &str) {
    let candidate = trim_candidate_answer(value);
    if candidate.is_empty() {
        return;
    }
    if !candidates.iter().any(|existing| existing == &candidate) {
        candidates.push(candidate);
    }
}

fn trim_candidate_answer(value: &str) -> String {
    let mut trimmed = value
        .trim()
        .trim_matches('"')
        .trim()
        .trim_start_matches("Answer:")
        .trim()
        .trim_end_matches('.')
        .trim()
        .to_string();
    if let Some((before_confidence, _)) = trimmed.split_once("Confidence:") {
        trimmed = before_confidence.trim().to_string();
    }
    trimmed
}

fn deduplicate_answers(values: Vec<String>) -> Vec<String> {
    let mut deduplicated = Vec::new();
    for value in values {
        if !deduplicated.iter().any(|existing| existing == &value) {
            deduplicated.push(value);
        }
    }
    deduplicated
}

fn answer_match_alternatives(value: &str) -> Vec<String> {
    let mut alternatives = Vec::new();
    push_normalized_alternative(&mut alternatives, value);

    if let Some((before_parentheses, after_parentheses)) = value.split_once('(') {
        push_normalized_alternative(&mut alternatives, before_parentheses);
        if let Some((inside_parentheses, _)) = after_parentheses.split_once(')') {
            for alias in alias_fragments(inside_parentheses) {
                push_normalized_alternative(&mut alternatives, alias);
            }
        }
    }

    alternatives
}

fn alias_fragments(value: &str) -> Vec<&str> {
    let mut aliases = Vec::new();
    for fragment in value.split(',') {
        let fragment = fragment.trim();
        let fragment = fragment
            .strip_prefix("also written as ")
            .or_else(|| fragment.strip_prefix("also referred to as "))
            .or_else(|| fragment.strip_prefix("also known as "))
            .or_else(|| fragment.strip_prefix("aka "))
            .unwrap_or(fragment)
            .trim();
        for alias in fragment.split(" or ") {
            aliases.push(alias.trim());
        }
    }
    aliases
}

fn push_normalized_alternative(alternatives: &mut Vec<String>, value: &str) {
    let normalized = normalize_for_answer_match(value);
    if normalized.is_empty() {
        return;
    }
    if !alternatives.iter().any(|existing| existing == &normalized) {
        alternatives.push(normalized);
    }
}

fn normalized_answers_match(expected: &str, candidate: &str) -> bool {
    if expected == candidate {
        return true;
    }
    let expected_tokens = expected.split_whitespace().count();
    let candidate_tokens = candidate.split_whitespace().count();
    if expected_tokens >= 2
        && candidate_tokens >= 2
        && (expected.contains(candidate) || candidate.contains(expected))
    {
        return true;
    }

    let loose_expected = remove_answer_glue_tokens(expected);
    let loose_candidate = remove_answer_glue_tokens(candidate);
    if loose_expected != expected || loose_candidate != candidate {
        let loose_expected_tokens = loose_expected.split_whitespace().count();
        let loose_candidate_tokens = loose_candidate.split_whitespace().count();
        return loose_expected_tokens >= 2
            && loose_candidate_tokens >= 2
            && (loose_expected == loose_candidate
                || loose_expected.contains(&loose_candidate)
                || loose_candidate.contains(&loose_expected));
    }

    false
}

fn normalize_for_answer_match(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut previous_was_space = true;
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch);
            previous_was_space = false;
        } else if !previous_was_space {
            normalized.push(' ');
            previous_was_space = true;
        }
    }
    normalized.trim().to_string()
}

fn remove_answer_glue_tokens(value: &str) -> String {
    value
        .split_whitespace()
        .filter(|token| !matches!(*token, "a" | "an" | "and" | "are" | "is" | "the"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn extracts_exact_answer_from_memoryarena_response() {
        let candidates = memoryarena_answer_candidates_from_text(
            "Explanation: evidence\n\n**Exact Answer:** Ihuoma Sonia Uche\n\nConfidence: 95%",
        );

        assert_eq!(candidates, vec!["Ihuoma Sonia Uche"]);
    }

    #[test]
    fn scores_when_gold_is_one_of_candidate_answers() {
        assert!(memoryarena_answers_match(
            "John Daniel delos Santos (also written as Daniel Delos Santos)",
            "Daniel Delos Santos",
        ));
    }

    #[test]
    fn scores_single_token_exact_answer_from_long_response() {
        let score = score_memoryarena_answer(
            "progressive_search",
            &json!("Manuelita"),
            Some("Explanation: evidence\n\nExact Answer: Manuelita\n\nConfidence: 85%"),
        );

        assert!(score.hard_success);
        assert!(score.candidate_answer_hit);
    }

    #[test]
    fn scores_equivalent_labeled_composite_answer() {
        let score = score_memoryarena_answer(
            "progressive_search",
            &json!("Poem: \"Namaste\" | Book: \"Almost Human\" by Thomas Centolella"),
            Some(
                "Explanation: evidence\n\nExact Answer: The poem is \"Namaste\" and the book is \"Almost Human\" by Thomas Centolella",
            ),
        );

        assert!(score.hard_success);
        assert!(score.candidate_answer_hit);
    }

    #[test]
    fn falls_back_to_normalized_text_when_no_exact_answer() {
        let candidates = memoryarena_answer_candidates_from_value(&json!("  plain answer. "));

        assert_eq!(candidates, vec!["plain answer"]);
    }

    #[test]
    fn scores_shopping_target_asin_inside_answer_text() {
        let score = score_memoryarena_answer(
            "bundled_shopping",
            &json!({
                "target_asin": "B00TUDFEW2",
                "attributes": ["Almond Flour", "Vanilla Cake Mix"]
            }),
            Some("The selected item is ASIN B00TUDFEW2 because it is an almond flour cake mix."),
        );

        assert!(score.hard_success);
        assert_eq!(score.expected_answer_kind, "target_asin");
        assert_eq!(score.soft_score, Some(0.5));
    }

    #[test]
    fn task_success_rule_matches_paper_domains() {
        assert_eq!(
            memoryarena_task_success_rule("bundled_shopping"),
            "all_subtasks_hard_success"
        );
        assert_eq!(
            memoryarena_task_success_rule("progressive_search"),
            "final_subtask_hard_success"
        );
        assert_eq!(
            memoryarena_task_success_rule("formal_reasoning_math"),
            "final_subtask_hard_success"
        );
        assert_eq!(
            memoryarena_task_success_rule("formal_reasoning_phys"),
            "final_subtask_hard_success"
        );
        assert_eq!(
            memoryarena_task_success_rule("group_travel_planner"),
            "all_subtasks_hard_success"
        );
    }

    #[test]
    fn scores_formal_reasoning_exact_answers() {
        let math_score = score_memoryarena_answer(
            "formal_reasoning_math",
            &json!("The correct answer is A,B,C"),
            Some("After applying the theorem, the correct answer is A,B,C."),
        );
        let phys_score = score_memoryarena_answer(
            "formal_reasoning_phys",
            &json!("All vector fields on Spec(A) have flow"),
            Some("Exact Answer: All vector fields on Spec(A) have flow"),
        );

        assert!(math_score.hard_success);
        assert!(phys_score.hard_success);
        assert_eq!(math_score.expected_answer_kind, "string");
        assert_eq!(phys_score.expected_answer_kind, "string");
    }

    #[test]
    fn computes_travel_slot_coverage_proxy() {
        let score = score_memoryarena_answer(
            "group_travel_planner",
            &json!([
                {
                    "days": 1,
                    "current_city": "Rockford",
                    "transportation": "Flight Number: F3573659",
                    "dinner": "Coco Bambu, Rockford",
                    "accommodation": "-"
                }
            ]),
            Some("Day 1 current city Rockford. Dinner: Coco Bambu, Rockford."),
        );

        assert!(score.soft_score.is_some_and(|score| score > 0.0));
        assert!(!score.hard_success);
        assert_eq!(
            score.soft_score_basis,
            Some("travel_expected_slot_text_coverage_proxy")
        );
    }
}
