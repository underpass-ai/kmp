use std::collections::{BTreeMap, BTreeSet};

use kmp_domain::TokenEstimator;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

pub(crate) const DEFAULT_MAX_BYTES: usize = 10_000;

/// Reads `budget.max_bytes` the way the recall projection does, so the
/// temporal verbs cannot mean something different by the same argument.
pub(crate) fn requested_byte_limit(arguments: &Value) -> Result<usize, String> {
    match arguments.pointer("/budget/max_bytes") {
        None | Some(Value::Null) => Ok(DEFAULT_MAX_BYTES),
        Some(value) => value
            .as_u64()
            .and_then(|bytes| usize::try_from(bytes).ok())
            .filter(|bytes| *bytes >= 512)
            .ok_or_else(|| "budget.max_bytes must be an integer of at least 512".to_string()),
    }
}
const CURSOR_VERSION: &str = "kmp1";
const PROJECTION_CONTRACT: &str = "kmp.recall.projection.v1";

#[derive(Debug)]
pub(super) enum ProjectionOutcome {
    Projected(Value),
    CoreTooLarge,
}

pub(super) fn project_recall_output(
    value: Value,
    arguments: &Value,
    default_tokens: u32,
    estimator: &dyn TokenEstimator,
) -> Result<ProjectionOutcome, String> {
    let budget = ProjectionBudget::from_arguments(arguments, default_tokens)?;
    let selection_hash = selection_hash(arguments, &value);
    let mut plan = ProjectionPlan::build(value, &budget);
    let eligible = plan
        .items
        .iter()
        .filter(|item| item.min_detail <= budget.detail)
        .cloned()
        .collect::<Vec<_>>();
    let excluded_by_detail = plan.items.len() - eligible.len();
    let offset = parse_cursor(
        arguments.pointer("/page/cursor").and_then(Value::as_str),
        &selection_hash,
        eligible.len(),
    )?;

    // Size the core against one worst-case metadata envelope so its bytes do
    // not change with detail, cursor offset, or an advisory-token override.
    let core_budget = ProjectionBudget {
        token_limit: u32::MAX,
        detail: Detail::Balanced,
        page_entries: usize::MAX,
        ..budget
    };
    let (core, core_text_shortened) = match fit_core(
        &plan,
        &plan.items,
        0,
        plan.items.len(),
        &selection_hash,
        &core_budget,
    ) {
        Some(core) => core,
        None => return Ok(ProjectionOutcome::CoreTooLarge),
    };
    plan.core = core;
    plan.core_lengths = section_lengths(&plan.core);

    let mut selected = Vec::new();
    let mut planning = plan.core.clone();
    attach_metadata(
        &mut planning,
        &plan,
        &eligible,
        &[],
        offset,
        excluded_by_detail,
        &selection_hash,
        &budget,
        core_text_shortened,
        true,
    );
    let mut planned_bytes = serialized_bytes(&planning);
    let mut planned_tokens = serialized_tokens(&planning, estimator);
    let mut lengths = plan.core_lengths.clone();

    for item in eligible
        .iter()
        .skip(offset)
        .take(budget.page_entries)
        .take_while(|_| !core_text_shortened)
    {
        let item_json =
            serde_json::to_string(&item.value).expect("projection item should serialize");
        let comma_bytes = usize::from(lengths.get(&item.section).copied().unwrap_or(0) > 0);
        let item_bytes = item_json.len() + comma_bytes;
        let item_tokens = estimator.estimate_tokens(&item_json).saturating_add(2);
        if planned_bytes.saturating_add(item_bytes) > budget.byte_limit
            || planned_tokens.saturating_add(item_tokens) > budget.token_limit
        {
            break;
        }
        planned_bytes += item_bytes;
        planned_tokens = planned_tokens.saturating_add(item_tokens);
        *lengths.entry(item.section).or_default() += 1;
        selected.push(item.clone());
    }

    // Item costs are deliberately conservative, so this normally runs once.
    // The exact final assertion protects the hard byte ceiling and estimator
    // compatibility without the old serialize-and-drop O(n²) loop.
    loop {
        let mut projected = plan.core.clone();
        for item in &selected {
            push_array(&mut projected, item.section.path(), item.value.clone());
        }
        attach_metadata(
            &mut projected,
            &plan,
            &eligible,
            &selected,
            offset,
            excluded_by_detail,
            &selection_hash,
            &budget,
            core_text_shortened,
            false,
        );
        stabilize_used_bytes(&mut projected);
        if fits(&projected, &budget) {
            return Ok(ProjectionOutcome::Projected(projected));
        }
        if selected.pop().is_none() {
            return Ok(ProjectionOutcome::CoreTooLarge);
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Detail {
    Compact,
    Balanced,
    Full,
}

impl Detail {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "compact" => Ok(Self::Compact),
            "balanced" => Ok(Self::Balanced),
            "full" => Ok(Self::Full),
            other => Err(format!("invalid budget.detail `{other}`")),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Balanced => "balanced",
            Self::Full => "full",
        }
    }
}

#[derive(Clone, Copy)]
struct ProjectionBudget {
    token_limit: u32,
    byte_limit: usize,
    detail: Detail,
    max_entries: Option<usize>,
    page_entries: usize,
}

impl ProjectionBudget {
    fn from_arguments(arguments: &Value, default_tokens: u32) -> Result<Self, String> {
        let budget = arguments.get("budget").and_then(Value::as_object);
        let token_limit = budget
            .and_then(|budget| budget.get("tokens"))
            .and_then(Value::as_u64)
            .and_then(|tokens| u32::try_from(tokens).ok())
            .filter(|tokens| *tokens > 0)
            .unwrap_or(default_tokens);
        let byte_limit = match budget.and_then(|budget| budget.get("max_bytes")) {
            None => DEFAULT_MAX_BYTES,
            Some(value) => value
                .as_u64()
                .and_then(|bytes| usize::try_from(bytes).ok())
                .filter(|bytes| *bytes >= 512)
                .ok_or_else(|| "budget.max_bytes must be an integer of at least 512".to_string())?,
        };
        let detail = Detail::parse(
            budget
                .and_then(|budget| budget.get("detail"))
                .and_then(Value::as_str)
                .unwrap_or("balanced"),
        )?;
        let max_entries = budget
            .and_then(|budget| budget.get("max_entries"))
            .and_then(Value::as_u64)
            .and_then(|entries| usize::try_from(entries).ok())
            .filter(|entries| *entries > 0);
        let page = match arguments.get("page") {
            None => None,
            Some(Value::Object(page)) => Some(page),
            Some(_) => return Err("page must be an object".to_string()),
        };
        let page_entries = match page.and_then(|page| page.get("entries")) {
            None => usize::MAX,
            Some(value) => value
                .as_u64()
                .and_then(|entries| usize::try_from(entries).ok())
                .filter(|entries| *entries > 0)
                .ok_or_else(|| "page.entries must be a positive integer".to_string())?,
        };
        if let Some(cursor) = page.and_then(|page| page.get("cursor"))
            && !cursor
                .as_str()
                .is_some_and(|cursor| !cursor.trim().is_empty())
        {
            return Err("page.cursor must be a non-empty opaque string".to_string());
        }
        Ok(Self {
            token_limit,
            byte_limit,
            detail,
            max_entries,
            page_entries,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Section {
    WakeCurrentState,
    WakeCausalSpine,
    WakeOpenLoops,
    WakeNextActions,
    WakeGuardrails,
    ProofEvidence,
    ProofPath,
    ProofMissing,
}

impl Section {
    const ALL: [Self; 8] = [
        Self::WakeCurrentState,
        Self::WakeCausalSpine,
        Self::WakeOpenLoops,
        Self::WakeNextActions,
        Self::WakeGuardrails,
        Self::ProofEvidence,
        Self::ProofPath,
        Self::ProofMissing,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::WakeCurrentState => "wake.current_state",
            Self::WakeCausalSpine => "wake.causal_spine",
            Self::WakeOpenLoops => "wake.open_loops",
            Self::WakeNextActions => "wake.next_actions",
            Self::WakeGuardrails => "wake.guardrails",
            Self::ProofEvidence => "proof.evidence",
            Self::ProofPath => "proof.path",
            Self::ProofMissing => "proof.missing",
        }
    }

    fn path(self) -> &'static [&'static str] {
        match self {
            Self::WakeCurrentState => &["wake", "current_state"],
            Self::WakeCausalSpine => &["wake", "causal_spine"],
            Self::WakeOpenLoops => &["wake", "open_loops"],
            Self::WakeNextActions => &["wake", "next_actions"],
            Self::WakeGuardrails => &["wake", "guardrails"],
            Self::ProofEvidence => &["proof", "evidence"],
            Self::ProofPath => &["proof", "path"],
            Self::ProofMissing => &["proof", "missing"],
        }
    }
}

#[derive(Clone)]
struct ProjectionItem {
    section: Section,
    value: Value,
    min_detail: Detail,
    priority: u8,
    stable_key: String,
}

struct ProjectionPlan {
    core: Value,
    items: Vec<ProjectionItem>,
    core_lengths: BTreeMap<Section, usize>,
    selection_omitted: usize,
}

impl ProjectionPlan {
    fn build(mut value: Value, budget: &ProjectionBudget) -> Self {
        let mut selection_omitted = 0usize;
        if let Some(max_entries) = budget.max_entries
            && let Some(reasons) = array_at_mut(&mut value, &["because"])
            && reasons.len() > max_entries
        {
            selection_omitted = reasons.len() - max_entries;
            reasons.truncate(max_entries);
            rebuild_answer(&mut value);
        }

        let mut items = Vec::new();
        if value.get("wake").is_some() {
            for (section, min_detail, priority) in [
                (Section::WakeCurrentState, Detail::Compact, 0),
                (Section::WakeCausalSpine, Detail::Compact, 5),
                (Section::WakeOpenLoops, Detail::Balanced, 10),
                (Section::WakeNextActions, Detail::Balanced, 11),
                (Section::WakeGuardrails, Detail::Balanced, 12),
            ] {
                let mut values = take_array(&mut value, section.path());
                if !values.is_empty() {
                    push_array(&mut value, section.path(), values.remove(0));
                }
                items.extend(
                    values
                        .into_iter()
                        .map(|value| ProjectionItem::new(section, value, min_detail, priority)),
                );
            }
        }

        let required_refs = cited_evidence_refs(&value)
            .union(&wake_evidence_refs(&value))
            .cloned()
            .collect::<BTreeSet<_>>();
        let evidence_items = take_array(&mut value, &["proof", "evidence"]);
        let required_count = evidence_items
            .iter()
            .filter(|evidence| {
                evidence
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| required_refs.contains(id))
            })
            .count();
        let extra_limit = budget
            .max_entries
            .map(|limit| limit.saturating_sub(required_count))
            .unwrap_or(usize::MAX);
        let mut extras_retained = 0usize;
        for evidence in evidence_items {
            let evidence_id = evidence
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if required_refs.contains(evidence_id) {
                push_array(&mut value, &["proof", "evidence"], evidence);
            } else if extras_retained < extra_limit {
                items.push(ProjectionItem::new(
                    Section::ProofEvidence,
                    evidence,
                    Detail::Balanced,
                    20,
                ));
                extras_retained += 1;
            } else {
                selection_omitted += 1;
            }
        }
        for relation in take_array(&mut value, &["proof", "path"]) {
            let (min_detail, priority) = relation_priority(&relation);
            items.push(ProjectionItem::new(
                Section::ProofPath,
                relation,
                min_detail,
                priority,
            ));
        }
        for missing in take_array(&mut value, &["proof", "missing"]) {
            items.push(ProjectionItem::new(
                Section::ProofMissing,
                missing,
                Detail::Full,
                90,
            ));
        }
        items.sort_by(|left, right| {
            (left.min_detail, left.priority, &left.stable_key).cmp(&(
                right.min_detail,
                right.priority,
                &right.stable_key,
            ))
        });
        let core_lengths = section_lengths(&value);
        Self {
            core: value,
            items,
            core_lengths,
            selection_omitted,
        }
    }
}

impl ProjectionItem {
    fn new(section: Section, value: Value, min_detail: Detail, priority: u8) -> Self {
        let stable_key = serde_json::to_string(&value).expect("projection item should serialize");
        Self {
            section,
            value,
            min_detail,
            priority,
            stable_key,
        }
    }
}

fn relation_priority(relation: &Value) -> (Detail, u8) {
    if relation.get("class").and_then(Value::as_str) == Some("structural") {
        return (Detail::Full, 80);
    }
    match relation
        .get("rel")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "supports" | "has_evidence" | "records" | "contains_entry" => (Detail::Balanced, 40),
        _ => (Detail::Compact, 10),
    }
}

fn fit_core(
    plan: &ProjectionPlan,
    eligible: &[ProjectionItem],
    offset: usize,
    excluded_by_detail: usize,
    selection_hash: &str,
    budget: &ProjectionBudget,
) -> Option<(Value, bool)> {
    let build = |max_chars: Option<usize>| {
        let mut candidate = plan.core.clone();
        if let Some(max_chars) = max_chars {
            truncate_json_text(&mut candidate, max_chars);
        }
        attach_metadata(
            &mut candidate,
            plan,
            eligible,
            &[],
            offset,
            excluded_by_detail,
            selection_hash,
            budget,
            max_chars.is_some(),
            true,
        );
        candidate
    };
    if fits(&build(None), budget) {
        return Some((plan.core.clone(), false));
    }

    let mut low = 0usize;
    let mut high = max_text_chars(&plan.core);
    let mut best = None;
    while low <= high {
        let midpoint = low + (high - low) / 2;
        if fits(&build(Some(midpoint)), budget) {
            let mut bounded = plan.core.clone();
            truncate_json_text(&mut bounded, midpoint);
            best = Some((bounded, true));
            low = midpoint.saturating_add(1);
        } else if midpoint == 0 {
            break;
        } else {
            high = midpoint - 1;
        }
    }
    best
}

#[allow(clippy::too_many_arguments)]
fn attach_metadata(
    value: &mut Value,
    plan: &ProjectionPlan,
    eligible: &[ProjectionItem],
    selected: &[ProjectionItem],
    offset: usize,
    excluded_by_detail: usize,
    selection_hash: &str,
    budget: &ProjectionBudget,
    core_text_shortened: bool,
    planning: bool,
) {
    const PLANNING_WARNING: &str = "pageable recall projection is partial; follow a non-null projection.page.next_cursor, combine continuation pages, or start a fresh recall with richer detail, larger max_entries, or larger max_bytes as indicated by truncation.omitted";
    let next_offset = offset.saturating_add(selected.len());
    let has_more = next_offset < eligible.len();
    let reported_offset = if planning { usize::MAX } else { offset };
    let cursor = if planning {
        make_cursor(usize::MAX, &"f".repeat(64))
    } else if has_more {
        make_cursor(next_offset, selection_hash)
    } else {
        String::new()
    };
    let mut sections = Map::new();
    for section in Section::ALL {
        let core = plan.core_lengths.get(&section).copied().unwrap_or(0);
        let total = core
            + plan
                .items
                .iter()
                .filter(|item| item.section == section)
                .count();
        if total == 0 {
            continue;
        }
        let eligible_total = core
            + eligible
                .iter()
                .filter(|item| item.section == section)
                .count();
        let returned = selected
            .iter()
            .filter(|item| item.section == section)
            .count();
        sections.insert(
            section.name().to_string(),
            json!({
                "core": core,
                "returned_on_page": if planning { total } else { returned },
                "eligible": eligible_total,
                "total": total
            }),
        );
    }
    let truncated = planning
        || has_more
        || offset > 0
        || excluded_by_detail > 0
        || plan.selection_omitted > 0
        || core_text_shortened;
    let next_action = if planning || has_more {
        format!(
            "Call the same recall tool with identical bound arguments and page.cursor=\"{cursor}\"; budget.tokens, budget.max_bytes, and page.entries may change."
        )
    } else {
        String::new()
    };
    value["projection"] = json!({
        "contract": PROJECTION_CONTRACT,
        "detail": budget.detail.as_str(),
        "budget": {
            "max_bytes": budget.byte_limit,
            "used_bytes": budget.byte_limit,
            "tokens_advisory": budget.token_limit
        },
        "page": {
            "offset": reported_offset,
            "returned": if planning { eligible.len() } else { selected.len() },
            "total": eligible.len(),
            "has_more": planning || has_more,
            "next_cursor": if cursor.is_empty() { Value::Null } else { json!(cursor) }
        },
        "sections": sections,
        "excluded_by_detail": excluded_by_detail,
        "selection_omitted": plan.selection_omitted,
        "core_text_shortened": core_text_shortened,
        "next_action": if next_action.is_empty() { Value::Null } else { json!(next_action) }
    });
    if truncated {
        let remaining_page_items = eligible.len().saturating_sub(next_offset);
        value["truncation"] = json!({
            "truncated": true,
            "token_limit": budget.token_limit,
            "byte_limit": budget.byte_limit,
            "omitted": {
                "page_items": if planning { eligible.len() } else { eligible.len().saturating_sub(selected.len()) },
                "prior_page_items": if planning { eligible.len() } else { offset },
                "remaining_page_items": if planning { eligible.len() } else { remaining_page_items },
                "excluded_by_detail": excluded_by_detail,
                "selection_items": plan.selection_omitted,
                "core_text_shortened": core_text_shortened
            }
        });
        let warning = if planning {
            PLANNING_WARNING
        } else if has_more {
            "pageable recall projection has more expansion items; follow the non-null projection.page.next_cursor with identical bound arguments"
        } else if excluded_by_detail > 0 {
            "recall detail excludes expansion items; start a fresh recall with a richer budget.detail to include them"
        } else if plan.selection_omitted > 0 {
            "recall selection was capped by budget.max_entries; start a fresh recall with a larger cap to include those items"
        } else if core_text_shortened {
            "recall core prose was shortened; start a fresh recall with a larger budget.max_bytes to restore it before expansion"
        } else {
            "final continuation page; combine its expansion items with the stable core and earlier pages"
        };
        append_warning(value, warning);
    } else if let Some(object) = value.as_object_mut() {
        object.remove("truncation");
    }
}

fn append_warning(value: &mut Value, warning: &str) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    let warnings = object
        .entry("warnings")
        .or_insert_with(|| json!([]))
        .as_array_mut();
    if let Some(warnings) = warnings
        && !warnings.iter().any(|item| item.as_str() == Some(warning))
    {
        warnings.push(json!(warning));
    }
}

fn selection_hash(arguments: &Value, value: &Value) -> String {
    let mut bound_arguments = arguments.clone();
    if let Some(arguments) = bound_arguments.as_object_mut() {
        arguments.remove("page");
        if let Some(budget) = arguments.get_mut("budget").and_then(Value::as_object_mut) {
            budget.remove("tokens");
            budget.remove("max_bytes");
        }
    }
    let mut hasher = Sha256::new();
    hasher.update(PROJECTION_CONTRACT.as_bytes());
    hasher.update(b"\0");
    hasher.update(
        serde_json::to_vec(&bound_arguments).expect("projection arguments should serialize"),
    );
    hasher.update(b"\0");
    hasher.update(serde_json::to_vec(value).expect("projection selection should serialize"));
    format!("{:x}", hasher.finalize())
}

fn parse_cursor(cursor: Option<&str>, selection_hash: &str, total: usize) -> Result<usize, String> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };
    let mut parts = cursor.split(':');
    let version = parts.next();
    let offset = parts.next();
    let hash = parts.next();
    if version != Some(CURSOR_VERSION) || parts.next().is_some() || hash != Some(selection_hash) {
        return Err("invalid page.cursor: it does not match this recall selection".to_string());
    }
    offset
        .and_then(|offset| offset.parse::<usize>().ok())
        .filter(|offset| *offset <= total)
        .ok_or_else(|| "invalid page.cursor: continuation offset is out of range".to_string())
}

fn make_cursor(offset: usize, selection_hash: &str) -> String {
    format!("{CURSOR_VERSION}:{offset}:{selection_hash}")
}

fn cited_evidence_refs(value: &Value) -> BTreeSet<String> {
    value
        .get("because")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|reason| reason.get("ref").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect()
}

fn wake_evidence_refs(value: &Value) -> BTreeSet<String> {
    value
        .pointer("/wake/causal_spine")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|claim| claim.get("evidence_ref").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect()
}

fn rebuild_answer(value: &mut Value) {
    let citations = value
        .get("because")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|reason| {
            let evidence_ref = reason.get("ref").and_then(Value::as_str)?.trim();
            let claim = reason
                .get("claim")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            Some(if claim.is_empty() {
                evidence_ref.to_string()
            } else {
                format!("{claim} [{evidence_ref}]")
            })
        })
        .collect::<Vec<_>>();
    value["answer"] = match citations.as_slice() {
        [] => Value::Null,
        [single] => json!(format!(
            "Memory answer supported by {single}; canonical text is in proof.evidence."
        )),
        many => json!(format!(
            "Retrieved for this question by term overlap; read proof.evidence and judge whether it answers:\n{}",
            many.iter()
                .map(|citation| format!("- {citation}"))
                .collect::<Vec<_>>()
                .join("\n")
        )),
    };
}

fn section_lengths(value: &Value) -> BTreeMap<Section, usize> {
    Section::ALL
        .into_iter()
        .map(|section| (section, array_len(value, section.path())))
        .collect()
}

fn array_len(value: &Value, path: &[&str]) -> usize {
    let mut current = value;
    for key in path {
        let Some(next) = current.get(*key) else {
            return 0;
        };
        current = next;
    }
    current.as_array().map(Vec::len).unwrap_or(0)
}

fn array_at_mut<'a>(value: &'a mut Value, path: &[&str]) -> Option<&'a mut Vec<Value>> {
    let mut current = value;
    for key in path {
        current = current.get_mut(*key)?;
    }
    current.as_array_mut()
}

fn take_array(value: &mut Value, path: &[&str]) -> Vec<Value> {
    array_at_mut(value, path)
        .map(std::mem::take)
        .unwrap_or_default()
}

fn push_array(value: &mut Value, path: &[&str], item: Value) {
    if let Some(array) = array_at_mut(value, path) {
        array.push(item);
    }
}

fn max_text_chars(value: &Value) -> usize {
    match value {
        Value::String(text) => text.chars().count(),
        Value::Array(items) => items.iter().map(max_text_chars).max().unwrap_or(0),
        Value::Object(object) => object
            .iter()
            .filter(|(key, _)| !is_reference_key(key))
            .map(|(_, value)| max_text_chars(value))
            .max()
            .unwrap_or(0),
        _ => 0,
    }
}

fn truncate_json_text(value: &mut Value, max_chars: usize) -> usize {
    match value {
        Value::String(text) => {
            let total = text.chars().count();
            if total <= max_chars {
                0
            } else {
                let mut bounded = text.chars().take(max_chars).collect::<String>();
                bounded.push('…');
                *text = bounded;
                total - max_chars
            }
        }
        Value::Array(items) => items
            .iter_mut()
            .map(|item| truncate_json_text(item, max_chars))
            .sum(),
        Value::Object(object) => object
            .iter_mut()
            .filter(|(key, _)| !is_reference_key(key))
            .map(|(_, value)| truncate_json_text(value, max_chars))
            .sum(),
        _ => 0,
    }
}

fn is_reference_key(key: &str) -> bool {
    matches!(
        key,
        "id" | "ref" | "claim" | "supports" | "source_ref" | "target_ref" | "evidence_refs"
    ) || key.ends_with("_ref")
        || key.ends_with("_refs")
}

fn stabilize_used_bytes(value: &mut Value) {
    for _ in 0..3 {
        let used = serialized_bytes(value);
        if value
            .pointer("/projection/budget/used_bytes")
            .and_then(Value::as_u64)
            == u64::try_from(used).ok()
        {
            break;
        }
        value["projection"]["budget"]["used_bytes"] = json!(used);
    }
}

fn fits(value: &Value, budget: &ProjectionBudget) -> bool {
    serialized_bytes(value) <= budget.byte_limit
}

fn serialized_bytes(value: &Value) -> usize {
    serde_json::to_vec(value)
        .expect("recall projection should serialize")
        .len()
}

fn serialized_tokens(value: &Value, estimator: &dyn TokenEstimator) -> u32 {
    estimator
        .estimate_tokens(&serde_json::to_string(value).expect("recall projection should serialize"))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use kmp_application::queries::cl100k_estimator::Cl100kEstimator;

    use super::*;

    #[test]
    fn cursor_rejects_changed_bound_arguments() {
        let estimator = Cl100kEstimator::new();
        let packet = fixture();
        let first = project_recall_output(
            packet.clone(),
            &json!({
                "about": "project:kmp",
                "question": "What is current?",
                "budget": {"tokens": 900, "max_bytes": 4_000, "detail": "full"},
                "page": {"entries": 1}
            }),
            2_400,
            &estimator,
        )
        .expect("first page")
        .projected();
        let cursor = first
            .pointer("/projection/page/next_cursor")
            .and_then(Value::as_str)
            .expect("continuation cursor");
        let error = project_recall_output(
            packet,
            &json!({
                "about": "project:kmp",
                "question": "A changed question",
                "budget": {"tokens": 900, "max_bytes": 4_000, "detail": "full"},
                "page": {"entries": 1, "cursor": cursor}
            }),
            2_400,
            &estimator,
        )
        .expect_err("changed bound arguments must invalidate cursor");
        assert!(error.contains("does not match"));
    }

    #[test]
    fn compact_1400_and_balanced_1800_keep_the_same_cited_core() {
        let packet = large_fixture(80);
        let compact = projected(
            packet.clone(),
            json!({
                "about": "project:kmp",
                "question": "Which storage engine is current?",
                "budget": {"tokens": 1400, "max_bytes": 10_000, "detail": "compact"}
            }),
        );
        let balanced = projected(
            packet,
            json!({
                "about": "project:kmp",
                "question": "Which storage engine is current?",
                "budget": {"tokens": 1800, "max_bytes": 10_000, "detail": "balanced"}
            }),
        );

        assert_eq!(compact["answer"], balanced["answer"]);
        assert_eq!(compact["because"], balanced["because"]);
        let compact_evidence = compact["proof"]["evidence"].as_array().expect("evidence");
        let balanced_evidence = balanced["proof"]["evidence"].as_array().expect("evidence");
        assert!(balanced_evidence.starts_with(compact_evidence));
        assert_eq!(compact["because"].as_array().expect("reasons").len(), 3);
    }

    #[test]
    fn cited_evidence_survives_max_entries_even_when_it_is_not_first() {
        let mut packet = fixture();
        packet["proof"]["evidence"]
            .as_array_mut()
            .expect("evidence")
            .reverse();
        let output = projected(
            packet,
            json!({
                "about": "project:kmp",
                "question": "What is current?",
                "budget": {
                    "tokens": 10_000,
                    "max_bytes": 20_000,
                    "detail": "full",
                    "max_entries": 1
                }
            }),
        );

        let evidence = output["proof"]["evidence"].as_array().expect("evidence");
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0]["id"], "evidence:a");
        assert_eq!(output["projection"]["selection_omitted"], 1);
    }

    #[test]
    fn core_is_identical_across_detail_modes_when_text_must_shorten() {
        let packet = large_fixture(30);
        let args = |detail: &str| {
            json!({
                "about": "project:kmp",
                "question": "Which storage engine is current?",
                "budget": {"tokens": 900, "max_bytes": 2_200, "detail": detail}
            })
        };
        let compact = projected(packet.clone(), args("compact"));
        let balanced = projected(packet.clone(), args("balanced"));
        let full = projected(packet, args("full"));

        assert_eq!(compact["projection"]["core_text_shortened"], true);
        for field in ["answer", "because"] {
            assert_eq!(compact[field], balanced[field]);
            assert_eq!(balanced[field], full[field]);
        }
        assert_eq!(compact["proof"]["evidence"], balanced["proof"]["evidence"]);
        assert_eq!(balanced["proof"]["evidence"], full["proof"]["evidence"]);
        assert!(
            compact["proof"]["path"]
                .as_array()
                .expect("path")
                .is_empty()
        );
        assert!(
            balanced["proof"]["path"]
                .as_array()
                .expect("path")
                .is_empty()
        );
        assert!(full["proof"]["path"].as_array().expect("path").is_empty());
    }

    #[test]
    fn detail_modes_are_nested_fieldsets() {
        let packet = fixture();
        let args = |detail: &str| {
            json!({
                "about": "project:kmp",
                "question": "What is current?",
                "budget": {"tokens": 10_000, "max_bytes": 40_000, "detail": detail}
            })
        };
        let compact = projected(packet.clone(), args("compact"));
        let balanced = projected(packet.clone(), args("balanced"));
        let full = projected(packet, args("full"));

        assert_eq!(compact["because"], balanced["because"]);
        assert_eq!(balanced["because"], full["because"]);
        assert!(relation_set(&compact).is_subset(&relation_set(&balanced)));
        assert!(relation_set(&balanced).is_subset(&relation_set(&full)));
        assert!(evidence_set(&compact).is_subset(&evidence_set(&balanced)));
        assert!(evidence_set(&balanced).is_subset(&evidence_set(&full)));
        assert!(relation_set(&compact).contains("depends_on"));
        assert!(relation_set(&balanced).contains("supports"));
        assert!(relation_set(&full).contains("contains_entry"));
    }

    #[test]
    fn pages_reconstruct_the_full_proof_without_changing_the_answer() {
        let packet = large_fixture(21);
        let base_arguments = json!({
            "about": "project:kmp",
            "question": "Which storage engine is current?",
            "budget": {"tokens": 30_000, "max_bytes": 100_000, "detail": "full"}
        });
        let complete = projected(packet.clone(), base_arguments.clone());
        let expected_evidence = evidence_set(&complete);
        let expected_relations = relation_values(&complete);
        let expected_missing = string_set(&complete, "/proof/missing");
        let expected_answer = complete["answer"].clone();
        let expected_because = complete["because"].clone();
        let expected_core_evidence =
            complete["proof"]["evidence"].as_array().expect("evidence")[..3].to_vec();

        let mut cursor = None;
        let mut evidence = BTreeSet::new();
        let mut relations = BTreeSet::new();
        let mut missing = BTreeSet::new();
        let mut pages = 0usize;
        loop {
            let mut arguments = base_arguments.clone();
            arguments["page"] = json!({"entries": 4});
            if let Some(cursor) = &cursor {
                arguments["page"]["cursor"] = json!(cursor);
            }
            let page = projected(packet.clone(), arguments);
            assert_eq!(page["answer"], expected_answer);
            assert_eq!(page["because"], expected_because);
            assert!(
                page["proof"]["evidence"]
                    .as_array()
                    .expect("evidence")
                    .starts_with(&expected_core_evidence)
            );
            evidence.extend(evidence_set(&page));
            relations.extend(relation_values(&page));
            missing.extend(string_set(&page, "/proof/missing"));
            pages += 1;

            if page["projection"]["page"]["has_more"] == false {
                assert!(page["projection"]["page"]["next_cursor"].is_null());
                assert_eq!(page["truncation"]["omitted"]["remaining_page_items"], 0);
                assert!(
                    page["warnings"]
                        .as_array()
                        .expect("warnings")
                        .iter()
                        .any(|warning| warning
                            .as_str()
                            .is_some_and(|warning| warning.contains("final continuation page")))
                );
                break;
            }
            cursor = Some(
                page["projection"]["page"]["next_cursor"]
                    .as_str()
                    .expect("opaque cursor")
                    .to_string(),
            );
            assert!(pages < 20, "cursor must make forward progress");
        }

        assert!(pages > 1);
        assert_eq!(evidence, expected_evidence);
        assert_eq!(relations, expected_relations);
        assert_eq!(missing, expected_missing);
    }

    #[test]
    fn byte_budget_sweep_is_monotone_deterministic_and_exactly_accounted() {
        let packet = large_fixture(120);
        let estimator = Cl100kEstimator::new();
        let mut previous_path = 0usize;
        for max_bytes in (3_000..=10_000).step_by(250) {
            let arguments = json!({
                "about": "project:kmp",
                "question": "Which storage engine is current?",
                "budget": {"tokens": 30_000, "max_bytes": max_bytes, "detail": "full"}
            });
            let outputs = (0..3)
                .map(|_| {
                    project_recall_output(packet.clone(), &arguments, 2_400, &estimator)
                        .expect("projection")
                        .projected()
                })
                .collect::<Vec<_>>();
            let serialized = outputs
                .iter()
                .map(|value| serde_json::to_vec(value).expect("serialized projection"))
                .collect::<Vec<_>>();
            assert!(serialized.windows(2).all(|pair| pair[0] == pair[1]));
            assert!(serialized[0].len() <= max_bytes);
            assert_eq!(
                outputs[0]["projection"]["budget"]["used_bytes"],
                serialized[0].len()
            );
            let path = outputs[0]["proof"]["path"].as_array().expect("path").len();
            assert!(path >= previous_path, "larger byte budget lost proof path");
            previous_path = path;
        }
    }

    #[test]
    fn projection_does_not_tokenize_the_full_packet_once_per_omitted_item() {
        let estimator = CountingEstimator::default();
        let packet = large_fixture(480);
        let output = project_recall_output(
            packet,
            &json!({
                "about": "project:kmp",
                "question": "Which storage engine is current?",
                "budget": {"tokens": 1200, "max_bytes": 5_000, "detail": "full"}
            }),
            2_400,
            &estimator,
        )
        .expect("projection")
        .projected();

        assert_eq!(output["truncation"]["truncated"], true);
        assert!(
            estimator.calls.load(Ordering::Relaxed) < 80,
            "the projection should tokenize a bounded prefix, not all 480 omitted hops"
        );
    }

    #[derive(Default)]
    struct CountingEstimator {
        calls: AtomicUsize,
        inner: Cl100kEstimator,
    }

    impl TokenEstimator for CountingEstimator {
        fn estimate_tokens(&self, text: &str) -> u32 {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.inner.estimate_tokens(text)
        }

        fn name(&self) -> &str {
            self.inner.name()
        }
    }

    impl ProjectionOutcome {
        fn projected(self) -> Value {
            match self {
                Self::Projected(value) => value,
                Self::CoreTooLarge => panic!("fixture core should fit"),
            }
        }
    }

    fn fixture() -> Value {
        json!({
            "summary": "Deterministic answer.",
            "answer": "Memory answer supported by claim:a [evidence:a]; canonical text is in proof.evidence.",
            "because": [{"claim": "claim:a", "ref": "evidence:a"}],
            "proof": {
                "path": [
                    {"from": "claim:a", "to": "claim:b", "rel": "depends_on", "class": "causal", "why": "semantic"},
                    {"from": "evidence:a", "to": "claim:a", "rel": "supports", "class": "evidential", "why": "support"},
                    {"from": "root", "to": "claim:a", "rel": "contains_entry", "class": "structural"}
                ],
                "evidence": [
                    {"id": "evidence:a", "supports": ["claim:a"], "text": "canonical body", "source": "test"},
                    {"id": "evidence:b", "supports": ["claim:b"], "text": "additional body", "source": "test"}
                ],
                "conflicts": [],
                "superseded": [],
                "missing": ["raw:detail"],
                "frontier_size": 0,
                "matched_terms": [],
                "matched_relations": [],
                "confidence": "high"
            },
            "warnings": []
        })
    }

    fn large_fixture(path_count: usize) -> Value {
        let reasons = (0..3)
            .map(|index| {
                json!({
                    "claim": format!("claim:{index}"),
                    "ref": format!("evidence:{index}")
                })
            })
            .collect::<Vec<_>>();
        let evidence = (0..8)
            .map(|index| {
                json!({
                    "id": format!("evidence:{index}"),
                    "supports": [format!("claim:{index}")],
                    "text": format!("Canonical storage evidence {index}: {}", "grounded detail ".repeat(8)),
                    "source": format!("source:{index}")
                })
            })
            .collect::<Vec<_>>();
        let path = (0..path_count)
            .map(|index| {
                let (rel, class) = match index % 3 {
                    0 => ("depends_on", "causal"),
                    1 => ("supports", "evidential"),
                    _ => ("contains_entry", "structural"),
                };
                json!({
                    "from": format!("node:{index:04}"),
                    "to": format!("claim:{}", index % 3),
                    "rel": rel,
                    "class": class,
                    "why": format!("Deterministic relation explanation {index}")
                })
            })
            .collect::<Vec<_>>();
        json!({
            "summary": "Deterministic memory answer from 3 evidence items.",
            "answer": "Retrieved for this question by term overlap; read proof.evidence and judge whether it answers:\n- claim:0 [evidence:0]\n- claim:1 [evidence:1]\n- claim:2 [evidence:2]",
            "because": reasons,
            "proof": {
                "path": path,
                "evidence": evidence,
                "conflicts": [],
                "superseded": [],
                "missing": ["raw:one", "raw:two"],
                "frontier_size": 2,
                "matched_terms": ["storage", "current"],
                "matched_relations": ["supports"],
                "confidence": "high"
            },
            "warnings": []
        })
    }

    fn projected(packet: Value, arguments: Value) -> Value {
        project_recall_output(packet, &arguments, 2_400, &Cl100kEstimator::new())
            .expect("projection")
            .projected()
    }

    fn relation_set(value: &Value) -> BTreeSet<String> {
        value
            .pointer("/proof/path")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|relation| relation.get("rel").and_then(Value::as_str))
            .map(ToString::to_string)
            .collect()
    }

    fn relation_values(value: &Value) -> BTreeSet<String> {
        value
            .pointer("/proof/path")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|relation| serde_json::to_string(relation).expect("relation"))
            .collect()
    }

    fn evidence_set(value: &Value) -> BTreeSet<String> {
        string_set(value, "/proof/evidence")
    }

    fn string_set(value: &Value, pointer: &str) -> BTreeSet<String> {
        value
            .pointer(pointer)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|item| serde_json::to_string(item).expect("projection item"))
            .collect()
    }
}
