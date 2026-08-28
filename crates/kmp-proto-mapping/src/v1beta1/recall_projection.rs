use std::collections::{BTreeMap, BTreeSet};

use kmp_application::queries::cl100k_estimator::Cl100kEstimator;
use kmp_domain::TokenEstimator;
use kmp_proto::v1beta1::{
    AnswerReason, AskRequest, AskResponse, DimensionScopeMode, DimensionSelection,
    DimensionSelectionMode, ExpiredMemory, MemoryConfidence, MemoryDetailLevel, MemoryEvidence,
    MemoryRelation, MemorySemanticClass, RecallCursorError as ProtoRecallCursorError,
    RecallCursorErrorReason, RecallOmitted, RecallProjection, RecallProjectionBudget,
    RecallProjectionPage, RecallProjectionSection, RecallTruncation, SupersededMemory,
    TemporalCoordinate, TemporalCursor, WakeClaim, WakeRequest, WakeResponse,
};
use prost_types::Timestamp;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

pub const DEFAULT_MAX_BYTES: usize = 10_000;

/// Reads `budget.max_bytes` the way the recall projection does, so the
/// temporal verbs cannot mean something different by the same argument.
pub fn requested_byte_limit(arguments: &Value) -> Result<usize, String> {
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
pub const PROJECTION_CONTRACT: &str = "kmp.recall.projection.v1";

#[derive(Debug)]
pub enum ProjectionOutcome {
    Projected(Value),
    CoreTooLarge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecallProjectionError {
    InvalidRequest(String),
    Cursor {
        reason: RecallCursorErrorReason,
        cursor: String,
        message: String,
    },
    CoreTooLarge,
}

impl RecallProjectionError {
    pub fn cursor_detail(&self) -> Option<ProtoRecallCursorError> {
        match self {
            Self::Cursor {
                reason,
                cursor,
                message,
            } => Some(ProtoRecallCursorError {
                reason: *reason as i32,
                cursor: cursor.clone(),
                message: message.clone(),
            }),
            _ => None,
        }
    }
}

impl std::fmt::Display for RecallProjectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(message) => formatter.write_str(message),
            Self::Cursor { message, .. } => formatter.write_str(message),
            Self::CoreTooLarge => formatter.write_str(
                "recall projection byte budget is smaller than the stable citation core; \
                 increase budget.max_bytes",
            ),
        }
    }
}

impl std::error::Error for RecallProjectionError {}

pub fn project_wake_response(
    response: WakeResponse,
    request: &WakeRequest,
) -> Result<WakeResponse, RecallProjectionError> {
    let arguments = wake_arguments(request);
    project_typed_recall(response, arguments, 1_600, wake_value, apply_wake_value)
}

pub fn project_ask_response(
    response: AskResponse,
    request: &AskRequest,
) -> Result<AskResponse, RecallProjectionError> {
    let arguments = ask_arguments(request);
    project_typed_recall(response, arguments, 2_400, ask_value, apply_ask_value)
}

fn project_typed_recall<T>(
    response: T,
    arguments: Value,
    default_tokens: u32,
    render: fn(&T) -> Value,
    apply: fn(T, &Value) -> T,
) -> Result<T, RecallProjectionError> {
    let value = render(&response);
    match project_recall_output_typed(value, &arguments, default_tokens, &Cl100kEstimator::new())? {
        ProjectionOutcome::Projected(value) => Ok(apply(response, &value)),
        ProjectionOutcome::CoreTooLarge => Err(RecallProjectionError::CoreTooLarge),
    }
}

pub fn project_recall_output(
    value: Value,
    arguments: &Value,
    default_tokens: u32,
    estimator: &dyn TokenEstimator,
) -> Result<ProjectionOutcome, String> {
    project_recall_output_typed(value, arguments, default_tokens, estimator)
        .map_err(|error| error.to_string())
}

fn project_recall_output_typed(
    value: Value,
    arguments: &Value,
    default_tokens: u32,
    _estimator: &dyn TokenEstimator,
) -> Result<ProjectionOutcome, RecallProjectionError> {
    let budget = ProjectionBudget::from_arguments(arguments, default_tokens)
        .map_err(RecallProjectionError::InvalidRequest)?;
    let mut plan = ProjectionPlan::build(value, &budget);
    let eligible = plan
        .items
        .iter()
        .filter(|item| item.min_detail <= budget.detail)
        .cloned()
        .collect::<Vec<_>>();
    let selection_hash = selection_hash(arguments, &plan, &eligible);
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
    let mut lengths = plan.core_lengths.clone();

    for item in eligible.iter().skip(offset).take(budget.page_entries) {
        let item_json =
            serde_json::to_string(&item.value).expect("projection item should serialize");
        let comma_bytes = usize::from(lengths.get(&item.section).copied().unwrap_or(0) > 0);
        let item_bytes = item_json.len() + comma_bytes;
        // `tokens` predates the transport-neutral byte contract and remains in
        // the API as a planning hint. Treating it as a second hard cap made a
        // typical stable core consume the entire default before any expansion
        // item was considered: compact, balanced and full then returned the
        // same payload while claiming different detail exclusions. The byte
        // ceiling is the normative host-safety boundary; detail and paging
        // choose the expansion inside it.
        if planned_bytes.saturating_add(item_bytes) > budget.byte_limit {
            break;
        }
        planned_bytes += item_bytes;
        *lengths.entry(item.section).or_default() += 1;
        selected.push(item.clone());
    }

    if selected.is_empty() && offset < eligible.len() {
        // Never manufacture a continuation that cannot advance. `fit_core`
        // reserves one item, so reaching this branch means the hard byte
        // ceiling cannot carry both the stable core and any expansion.
        return Ok(ProjectionOutcome::CoreTooLarge);
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
    // A continuation is only useful if it can advance. Reserve enough room
    // for the largest expansion item in the selection while fitting the
    // stable core; otherwise a shortened core can consume the whole budget
    // and produce returned=0, has_more=true, and the same cursor forever.
    // Choosing from the complete canonical plan keeps this core identical
    // across detail levels and page offsets.
    let reserved_item = eligible.iter().max_by_key(|item| {
        item.stable_key.len()
            + usize::from(plan.core_lengths.get(&item.section).copied().unwrap_or(0) > 0)
    });
    let build = |max_chars: Option<usize>| {
        let mut candidate = plan.core.clone();
        if let Some(max_chars) = max_chars {
            truncate_json_text(&mut candidate, max_chars);
        }
        if let Some(item) = reserved_item {
            push_array(&mut candidate, item.section.path(), item.value.clone());
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

fn selection_hash(arguments: &Value, plan: &ProjectionPlan, eligible: &[ProjectionItem]) -> String {
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
    // Bind the cursor to the canonical projection plan, not the raw response
    // serialization returned by a graph adapter. Storage is allowed to return
    // equal-ranked rows in any order; ProjectionPlan gives expansion items a
    // total semantic order before this identity is computed. Hash the stable
    // core plus that ordered selection so identical snapshots produce the
    // same cursor across requests and transports, while any eligible semantic
    // change still invalidates it.
    hasher.update(
        serde_json::to_vec(&plan.core).expect("projection core should serialize canonically"),
    );
    for item in eligible {
        hasher.update(b"\0");
        hasher.update(item.section.name().as_bytes());
        hasher.update(b"\0");
        hasher.update(item.stable_key.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn parse_cursor(
    cursor: Option<&str>,
    selection_hash: &str,
    total: usize,
) -> Result<usize, RecallProjectionError> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };
    let mut parts = cursor.split(':');
    let version = parts.next();
    let offset = parts.next();
    let hash = parts.next();
    if version != Some(CURSOR_VERSION)
        || offset.is_none()
        || hash.is_none()
        || parts.next().is_some()
    {
        return Err(cursor_error(
            RecallCursorErrorReason::Malformed,
            cursor,
            "invalid page.cursor: malformed recall continuation",
        ));
    }
    if hash != Some(selection_hash) {
        return Err(cursor_error(
            RecallCursorErrorReason::SelectionChanged,
            cursor,
            "invalid page.cursor: it does not match this recall selection",
        ));
    }
    let offset = offset
        .and_then(|offset| offset.parse::<usize>().ok())
        .ok_or_else(|| {
            cursor_error(
                RecallCursorErrorReason::Malformed,
                cursor,
                "invalid page.cursor: malformed recall continuation offset",
            )
        })?;
    if offset > total {
        return Err(cursor_error(
            RecallCursorErrorReason::OffsetOutOfRange,
            cursor,
            "invalid page.cursor: continuation offset is out of range",
        ));
    }
    Ok(offset)
}

fn cursor_error(
    reason: RecallCursorErrorReason,
    cursor: &str,
    message: &str,
) -> RecallProjectionError {
    RecallProjectionError::Cursor {
        reason,
        cursor: cursor.to_string(),
        message: message.to_string(),
    }
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
        "id" | "ref"
            | "claim"
            | "supports"
            | "source_ref"
            | "target_ref"
            | "evidence_refs"
            | "superseded_by"
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

fn wake_arguments(request: &WakeRequest) -> Value {
    let mut arguments = Map::new();
    arguments.insert("about".to_string(), json!(request.about));
    insert_non_empty(&mut arguments, "role", &request.role);
    insert_non_empty(&mut arguments, "intent", &request.intent);
    arguments.insert(
        "budget".to_string(),
        budget_value(request.budget.as_ref(), 1_600, 2),
    );
    if let Some(dimensions) = request.dimensions.as_ref() {
        arguments.insert(
            "dimensions".to_string(),
            dimension_selection_value(dimensions),
        );
    }
    if let Some(page) = request.page.as_ref() {
        arguments.insert("page".to_string(), page_request_value(page));
    }
    Value::Object(arguments)
}

fn ask_arguments(request: &AskRequest) -> Value {
    let mut arguments = Map::new();
    arguments.insert("about".to_string(), json!(request.about));
    arguments.insert("question".to_string(), json!(request.question));
    arguments.insert(
        "answer_policy".to_string(),
        json!(
            match kmp_proto::v1beta1::AnswerPolicy::try_from(request.answer_policy) {
                Ok(kmp_proto::v1beta1::AnswerPolicy::ShowConflicts) => "show_conflicts",
                Ok(kmp_proto::v1beta1::AnswerPolicy::BestEffort) => "best_effort",
                _ => "evidence_or_unknown",
            }
        ),
    );
    arguments.insert(
        "budget".to_string(),
        budget_value(request.budget.as_ref(), 2_400, 2),
    );
    if let Some(dimensions) = request.dimensions.as_ref() {
        arguments.insert(
            "dimensions".to_string(),
            dimension_selection_value(dimensions),
        );
    }
    if let Some(page) = request.page.as_ref() {
        arguments.insert("page".to_string(), page_request_value(page));
    }
    Value::Object(arguments)
}

fn budget_value(
    budget: Option<&kmp_proto::v1beta1::MemoryBudget>,
    default_tokens: u32,
    default_depth: u32,
) -> Value {
    let budget = budget.cloned().unwrap_or_default();
    json!({
        "tokens": if budget.tokens == 0 { default_tokens } else { budget.tokens },
        "detail": detail_label(budget.detail),
        "depth": if budget.depth == 0 { default_depth } else { budget.depth },
        "max_entries": budget.max_entries,
        "max_bytes": if budget.max_bytes == 0 {
            DEFAULT_MAX_BYTES as u64
        } else {
            budget.max_bytes
        }
    })
}

fn dimension_selection_value(selection: &DimensionSelection) -> Value {
    json!({
        "mode": match DimensionSelectionMode::try_from(selection.mode) {
            Ok(DimensionSelectionMode::Only) => "only",
            Ok(DimensionSelectionMode::Except) => "except",
            _ => "all",
        },
        "include": selection.include,
        "exclude": selection.exclude,
        "scope": match DimensionScopeMode::try_from(selection.scope) {
            Ok(DimensionScopeMode::Abouts) => "abouts",
            Ok(DimensionScopeMode::AllAbouts) => "all_abouts",
            _ => "current_about",
        },
        "abouts": selection.abouts,
        "scope_ids": selection.scope_ids
    })
}

fn page_request_value(page: &kmp_proto::v1beta1::PageRequest) -> Value {
    let mut value = Map::new();
    if page.entries != 0 {
        value.insert("entries".to_string(), json!(page.entries));
    }
    insert_non_empty(&mut value, "cursor", &page.cursor);
    Value::Object(value)
}

pub fn wake_value(response: &WakeResponse) -> Value {
    let wake = response.wake.as_ref();
    let mut value = json!({
        "summary": response.summary,
        "wake": {
            "objective": wake.map(|wake| wake.objective.as_str()).unwrap_or(""),
            "current_state": wake.map(|wake| wake.current_state.clone()).unwrap_or_default(),
            "causal_spine": wake
                .map(|wake| wake.causal_spine.iter().map(wake_claim_value).collect::<Vec<_>>())
                .unwrap_or_default(),
            "open_loops": wake.map(|wake| wake.open_loops.clone()).unwrap_or_default(),
            "next_actions": wake.map(|wake| wake.next_actions.clone()).unwrap_or_default(),
            "guardrails": wake.map(|wake| wake.guardrails.clone()).unwrap_or_default()
        },
        "proof": response.proof.as_ref().map(proof_value).unwrap_or_else(empty_proof_value),
        "resume_cursor": response.resume_cursor.as_ref().map(temporal_cursor_value).unwrap_or(Value::Null),
        "warnings": response.warnings
    });
    attach_typed_projection(
        &mut value,
        response.projection.as_ref(),
        response.truncation.as_ref(),
    );
    value
}

pub fn ask_value(response: &AskResponse) -> Value {
    let evidence = response
        .proof
        .as_ref()
        .map(|proof| proof.evidence.as_slice())
        .unwrap_or_default();
    let answer = normalized_ask_answer(&response.answer, &response.because, evidence);
    let mut value = json!({
        "summary": response.summary,
        "answer": if answer.trim().is_empty() { Value::Null } else { Value::String(answer) },
        "because": response.because.iter().map(|reason| answer_reason_value(reason, evidence)).collect::<Vec<_>>(),
        "proof": response.proof.as_ref().map(proof_value).unwrap_or_else(empty_proof_value),
        "warnings": response.warnings
    });
    attach_typed_projection(
        &mut value,
        response.projection.as_ref(),
        response.truncation.as_ref(),
    );
    value
}

fn apply_wake_value(mut response: WakeResponse, value: &Value) -> WakeResponse {
    response.summary = string_at(value, "/summary");
    if let Some(wake) = response.wake.as_mut() {
        wake.objective = string_at(value, "/wake/objective");
        wake.current_state = strings_at(value, "/wake/current_state");
        wake.open_loops = strings_at(value, "/wake/open_loops");
        wake.next_actions = strings_at(value, "/wake/next_actions");
        wake.guardrails = strings_at(value, "/wake/guardrails");
        wake.causal_spine = value
            .pointer("/wake/causal_spine")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|claim| WakeClaim {
                claim: string_at(claim, "/claim"),
                because: string_at(claim, "/because"),
                evidence_ref: string_at(claim, "/evidence_ref"),
            })
            .collect();
    }
    if let Some(proof) = response.proof.as_mut()
        && let Some(projected) = value.get("proof")
    {
        apply_proof_value(proof, projected);
    }
    response.warnings = strings_at(value, "/warnings");
    response.projection = value.get("projection").and_then(projection_from_value);
    response.truncation = value.get("truncation").and_then(truncation_from_value);
    response
}

fn apply_ask_value(mut response: AskResponse, value: &Value) -> AskResponse {
    response.summary = string_at(value, "/summary");
    response.answer = value
        .get("answer")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let projected_reasons = value
        .get("because")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let evidence = response
        .proof
        .as_ref()
        .map(|proof| proof.evidence.as_slice())
        .unwrap_or_default();
    let normalized = response
        .because
        .iter()
        .map(|reason| normalized_answer_reason(reason, evidence))
        .collect::<Vec<_>>();
    response.because = select_projected(&normalized, &projected_reasons, |reason| {
        answer_reason_value(reason, evidence)
    });
    if let Some(proof) = response.proof.as_mut()
        && let Some(projected) = value.get("proof")
    {
        apply_proof_value(proof, projected);
    }
    response.warnings = strings_at(value, "/warnings");
    response.projection = value.get("projection").and_then(projection_from_value);
    response.truncation = value.get("truncation").and_then(truncation_from_value);
    response
}

fn apply_proof_value(proof: &mut kmp_proto::v1beta1::Proof, value: &Value) {
    let normalized_relations = proof
        .path
        .iter()
        .map(|relation| normalized_proof_relation(relation, &proof.evidence))
        .collect::<Vec<_>>();
    let path = value
        .get("path")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    proof.path = select_projected(&normalized_relations, &path, memory_relation_value);
    let evidence = value
        .get("evidence")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    proof.evidence = select_projected(&proof.evidence, &evidence, memory_evidence_value);
    let superseded = value
        .get("superseded")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    proof.superseded = select_projected_superseded(&proof.superseded, &superseded);
    let expired = value
        .get("expired")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    proof.expired = select_projected(&proof.expired, &expired, expired_value);
    proof.conflicts = strings_at(value, "/conflicts");
    proof.missing = strings_at(value, "/missing");
    proof.matched_terms = strings_at(value, "/matched_terms");
    proof.matched_relations = strings_at(value, "/matched_relations");
    proof.frontier_size = value
        .get("frontier_size")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or_default();
    proof.confidence = confidence_from_label(
        value
            .get("confidence")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
    );
}

fn select_projected<T: Clone>(
    originals: &[T],
    projected: &[Value],
    render: impl Fn(&T) -> Value,
) -> Vec<T> {
    let mut used = vec![false; originals.len()];
    projected
        .iter()
        .filter_map(|wanted| {
            let index = originals
                .iter()
                .enumerate()
                .find(|(index, item)| !used[*index] && render(item) == *wanted)
                .map(|(index, _)| index)?;
            used[index] = true;
            Some(originals[index].clone())
        })
        .collect()
}

fn select_projected_superseded(
    originals: &[SupersededMemory],
    projected: &[Value],
) -> Vec<SupersededMemory> {
    let mut used = vec![false; originals.len()];
    projected
        .iter()
        .filter_map(|wanted| {
            let r#ref = wanted.get("ref").and_then(Value::as_str)?;
            let superseded_by = wanted.get("superseded_by").and_then(Value::as_str)?;
            let index = originals
                .iter()
                .enumerate()
                .find(|(index, item)| {
                    !used[*index] && item.r#ref == r#ref && item.superseded_by == superseded_by
                })
                .map(|(index, _)| index)?;
            used[index] = true;
            let mut selected = originals[index].clone();
            selected.why = wanted
                .get("why")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            Some(selected)
        })
        .collect()
}

fn attach_typed_projection(
    value: &mut Value,
    projection: Option<&RecallProjection>,
    truncation: Option<&RecallTruncation>,
) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    if let Some(projection) = projection {
        object.insert("projection".to_string(), projection_value(projection));
    }
    if let Some(truncation) = truncation {
        object.insert("truncation".to_string(), truncation_value(truncation));
    }
}

fn projection_value(projection: &RecallProjection) -> Value {
    let mut sections = Map::new();
    for section in &projection.sections {
        sections.insert(
            section.name.clone(),
            json!({
                "core": section.core,
                "returned_on_page": section.returned_on_page,
                "eligible": section.eligible,
                "total": section.total
            }),
        );
    }
    let budget = projection.budget.unwrap_or_default();
    let page = projection.page.clone().unwrap_or_default();
    json!({
        "contract": projection.contract,
        "detail": detail_label(projection.detail),
        "budget": {
            "max_bytes": budget.max_bytes,
            "used_bytes": budget.used_bytes,
            "tokens_advisory": budget.tokens_advisory
        },
        "page": {
            "offset": page.offset,
            "returned": page.returned,
            "total": page.total,
            "has_more": page.has_more,
            "next_cursor": page.next_cursor.clone().map(Value::String).unwrap_or(Value::Null)
        },
        "sections": sections,
        "excluded_by_detail": projection.excluded_by_detail,
        "selection_omitted": projection.selection_omitted,
        "core_text_shortened": projection.core_text_shortened,
        "next_action": projection.next_action.clone().map(Value::String).unwrap_or(Value::Null)
    })
}

fn truncation_value(truncation: &RecallTruncation) -> Value {
    let omitted = truncation.omitted.unwrap_or_default();
    json!({
        "truncated": truncation.truncated,
        "token_limit": truncation.token_limit,
        "byte_limit": truncation.byte_limit,
        "omitted": {
            "page_items": omitted.page_items,
            "prior_page_items": omitted.prior_page_items,
            "remaining_page_items": omitted.remaining_page_items,
            "excluded_by_detail": omitted.excluded_by_detail,
            "selection_items": omitted.selection_items,
            "core_text_shortened": omitted.core_text_shortened
        }
    })
}

fn projection_from_value(value: &Value) -> Option<RecallProjection> {
    let sections = value
        .get("sections")?
        .as_object()?
        .iter()
        .map(|(name, section)| RecallProjectionSection {
            name: name.clone(),
            core: u64_at(section, "/core"),
            returned_on_page: u64_at(section, "/returned_on_page"),
            eligible: u64_at(section, "/eligible"),
            total: u64_at(section, "/total"),
        })
        .collect();
    Some(RecallProjection {
        contract: string_at(value, "/contract"),
        detail: detail_from_label(value.get("detail")?.as_str()?),
        budget: Some(RecallProjectionBudget {
            max_bytes: u64_at(value, "/budget/max_bytes"),
            used_bytes: u64_at(value, "/budget/used_bytes"),
            tokens_advisory: u32_at(value, "/budget/tokens_advisory"),
        }),
        page: Some(RecallProjectionPage {
            offset: u64_at(value, "/page/offset"),
            returned: u64_at(value, "/page/returned"),
            total: u64_at(value, "/page/total"),
            has_more: value
                .pointer("/page/has_more")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            next_cursor: value
                .pointer("/page/next_cursor")
                .and_then(Value::as_str)
                .map(ToString::to_string),
        }),
        sections,
        excluded_by_detail: u64_at(value, "/excluded_by_detail"),
        selection_omitted: u64_at(value, "/selection_omitted"),
        core_text_shortened: value
            .get("core_text_shortened")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        next_action: value
            .get("next_action")
            .and_then(Value::as_str)
            .map(ToString::to_string),
    })
}

fn truncation_from_value(value: &Value) -> Option<RecallTruncation> {
    Some(RecallTruncation {
        truncated: value.get("truncated")?.as_bool()?,
        token_limit: u32_at(value, "/token_limit"),
        byte_limit: u64_at(value, "/byte_limit"),
        omitted: Some(RecallOmitted {
            page_items: u64_at(value, "/omitted/page_items"),
            prior_page_items: u64_at(value, "/omitted/prior_page_items"),
            remaining_page_items: u64_at(value, "/omitted/remaining_page_items"),
            excluded_by_detail: u64_at(value, "/omitted/excluded_by_detail"),
            selection_items: u64_at(value, "/omitted/selection_items"),
            core_text_shortened: value
                .pointer("/omitted/core_text_shortened")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
    })
}

fn proof_value(proof: &kmp_proto::v1beta1::Proof) -> Value {
    json!({
        "path": proof.path.iter().map(|relation| memory_relation_value(&normalized_proof_relation(relation, &proof.evidence))).collect::<Vec<_>>(),
        "evidence": proof.evidence.iter().map(memory_evidence_value).collect::<Vec<_>>(),
        "conflicts": proof.conflicts,
        "superseded": proof.superseded.iter().map(superseded_value).collect::<Vec<_>>(),
        "expired": proof.expired.iter().map(expired_value).collect::<Vec<_>>(),
        "missing": proof.missing,
        "frontier_size": proof.frontier_size,
        "matched_terms": proof.matched_terms,
        "matched_relations": proof.matched_relations,
        "confidence": confidence_label(proof.confidence)
    })
}

fn normalized_proof_relation(
    relation: &MemoryRelation,
    evidence: &[MemoryEvidence],
) -> MemoryRelation {
    let mut relation = relation.clone();
    let mut refs = relation
        .evidence_refs
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut repeated_why = false;
    let mut repeated_evidence = false;
    for item in evidence {
        let evidence_node_ref = item.id.strip_prefix("detail:").unwrap_or(&item.id);
        let incident = relation.source_ref == evidence_node_ref
            || relation.target_ref == evidence_node_ref
            || item.supports.iter().any(|supported_ref| {
                relation.source_ref == *supported_ref || relation.target_ref == *supported_ref
            });
        let why_matches = !relation.why.is_empty() && relation.why == item.text;
        let evidence_matches = !relation.evidence.is_empty() && relation.evidence == item.text;
        if incident || why_matches || evidence_matches {
            refs.insert(item.id.clone());
        }
        repeated_why |= why_matches;
        repeated_evidence |= evidence_matches;
    }
    if repeated_why {
        relation.why.clear();
    }
    if repeated_evidence {
        relation.evidence.clear();
    }
    relation.evidence_refs = refs.into_iter().collect();
    if relation.semantic_class != MemorySemanticClass::Structural as i32
        && relation.why.is_empty()
        && relation.evidence.is_empty()
        && !relation.evidence_refs.is_empty()
    {
        relation.why = "Supported by canonical evidence refs.".to_string();
    }
    relation
}

fn empty_proof_value() -> Value {
    json!({
        "path": [], "evidence": [], "conflicts": [], "superseded": [], "expired": [],
        "missing": ["proof"], "frontier_size": 1, "matched_terms": [],
        "matched_relations": [], "confidence": "unknown"
    })
}

fn wake_claim_value(claim: &WakeClaim) -> Value {
    json!({"claim": claim.claim, "because": claim.because, "evidence_ref": claim.evidence_ref})
}

fn normalized_answer_reason(reason: &AnswerReason, evidence: &[MemoryEvidence]) -> AnswerReason {
    let mut reason = reason.clone();
    if evidence.iter().any(|item| {
        item.id == reason.r#ref && !item.text.is_empty() && item.text == reason.evidence
    }) {
        reason.evidence.clear();
    }
    reason
}

fn answer_reason_value(reason: &AnswerReason, evidence: &[MemoryEvidence]) -> Value {
    let reason = normalized_answer_reason(reason, evidence);
    let mut value = Map::new();
    value.insert("claim".to_string(), json!(reason.claim));
    insert_non_empty(&mut value, "evidence", &reason.evidence);
    value.insert("ref".to_string(), json!(reason.r#ref));
    Value::Object(value)
}

fn normalized_ask_answer(
    answer: &str,
    reasons: &[AnswerReason],
    evidence: &[MemoryEvidence],
) -> String {
    let repeats_canonical_body = evidence.iter().any(|item| {
        let body = item.text.trim();
        !body.is_empty() && answer.contains(body)
    });
    if !repeats_canonical_body {
        return answer.to_string();
    }
    let mut seen = BTreeSet::new();
    let citations = reasons
        .iter()
        .filter_map(|reason| {
            let evidence_ref = reason.r#ref.trim();
            if evidence_ref.is_empty() || !seen.insert(evidence_ref.to_string()) {
                return None;
            }
            let claim = reason.claim.trim();
            Some(if claim.is_empty() {
                evidence_ref.to_string()
            } else {
                format!("{claim} [{evidence_ref}]")
            })
        })
        .collect::<Vec<_>>();
    match citations.as_slice() {
        [] => String::new(),
        [single] => format!(
            "Retrieved for this question by term overlap; read proof.evidence and judge whether \
             it answers: {single}"
        ),
        many => format!(
            "Retrieved for this question by term overlap; read proof.evidence and judge whether it answers:\n{}",
            many.iter()
                .map(|item| format!("- {item}"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    }
}

fn superseded_value(entry: &SupersededMemory) -> Value {
    let mut value = Map::new();
    value.insert("ref".to_string(), json!(entry.r#ref));
    value.insert("superseded_by".to_string(), json!(entry.superseded_by));
    insert_non_empty(&mut value, "why", &entry.why);
    Value::Object(value)
}

fn expired_value(entry: &ExpiredMemory) -> Value {
    let mut value = Map::new();
    value.insert("ref".to_string(), json!(entry.r#ref));
    insert_timestamp(&mut value, "valid_until", entry.valid_until);
    Value::Object(value)
}

fn memory_relation_value(relation: &MemoryRelation) -> Value {
    let mut value = Map::new();
    value.insert("from".to_string(), json!(relation.source_ref));
    value.insert("to".to_string(), json!(relation.target_ref));
    value.insert("rel".to_string(), json!(relation.rel));
    value.insert(
        "class".to_string(),
        json!(semantic_class_label(relation.semantic_class)),
    );
    insert_non_empty(&mut value, "why", &relation.why);
    insert_non_empty(&mut value, "evidence", &relation.evidence);
    value.insert(
        "confidence".to_string(),
        json!(confidence_label(relation.confidence)),
    );
    if let Some(sequence) = relation.sequence {
        value.insert("sequence".to_string(), json!(sequence));
    }
    if let Some(explanation) = relation.explanation.as_ref() {
        insert_non_empty(&mut value, "motivation", &explanation.motivation);
        insert_non_empty(&mut value, "method", &explanation.method);
        insert_non_empty(&mut value, "decision_id", &explanation.decision_id);
        insert_non_empty(
            &mut value,
            "caused_by_node_id",
            &explanation.caused_by_node_id,
        );
        if let Some(coordinate) = explanation.coordinate.as_ref() {
            value.insert(
                "coordinate".to_string(),
                temporal_coordinate_value(coordinate),
            );
        }
    }
    if !relation.evidence_refs.is_empty() {
        value.insert("evidence_refs".to_string(), json!(relation.evidence_refs));
    }
    Value::Object(value)
}

fn memory_evidence_value(evidence: &MemoryEvidence) -> Value {
    let mut value = Map::new();
    value.insert("id".to_string(), json!(evidence.id));
    value.insert("supports".to_string(), json!(evidence.supports));
    value.insert("text".to_string(), json!(evidence.text));
    insert_non_empty(&mut value, "source", &evidence.source);
    insert_timestamp(&mut value, "time", evidence.time);
    if !evidence.metadata.is_empty() {
        value.insert("metadata".to_string(), json!(evidence.metadata));
    }
    Value::Object(value)
}

fn temporal_cursor_value(cursor: &TemporalCursor) -> Value {
    let mut value = Map::new();
    insert_non_empty(&mut value, "ref", &cursor.r#ref);
    insert_timestamp(&mut value, "time", cursor.time);
    if let Some(sequence) = cursor.sequence {
        value.insert("sequence".to_string(), json!(sequence));
    }
    Value::Object(value)
}

fn temporal_coordinate_value(coordinate: &TemporalCoordinate) -> Value {
    let mut value = Map::new();
    insert_non_empty(&mut value, "dimension", &coordinate.dimension);
    insert_non_empty(&mut value, "scope_id", &coordinate.scope_id);
    insert_timestamp(&mut value, "occurred_at", coordinate.occurred_at);
    insert_timestamp(&mut value, "observed_at", coordinate.observed_at);
    insert_timestamp(&mut value, "ingested_at", coordinate.ingested_at);
    insert_timestamp(&mut value, "valid_from", coordinate.valid_from);
    insert_timestamp(&mut value, "valid_until", coordinate.valid_until);
    if let Some(sequence) = coordinate.sequence {
        value.insert("sequence".to_string(), json!(sequence));
    }
    if let Some(rank) = coordinate.rank {
        value.insert("rank".to_string(), json!(rank));
    }
    if !coordinate.metadata.is_empty() {
        value.insert("metadata".to_string(), json!(coordinate.metadata));
    }
    Value::Object(value)
}

fn insert_non_empty(object: &mut Map<String, Value>, key: &str, value: &str) {
    if !value.trim().is_empty() {
        object.insert(key.to_string(), json!(value));
    }
}

fn insert_timestamp(object: &mut Map<String, Value>, key: &str, value: Option<Timestamp>) {
    if let Some(value) = value {
        object.insert(key.to_string(), json!(value.to_string()));
    }
}

fn detail_label(value: i32) -> &'static str {
    match MemoryDetailLevel::try_from(value) {
        Ok(MemoryDetailLevel::Compact) => "compact",
        Ok(MemoryDetailLevel::Full) => "full",
        _ => "balanced",
    }
}

fn detail_from_label(value: &str) -> i32 {
    match value {
        "compact" => MemoryDetailLevel::Compact as i32,
        "full" => MemoryDetailLevel::Full as i32,
        _ => MemoryDetailLevel::Balanced as i32,
    }
}

fn semantic_class_label(value: i32) -> &'static str {
    match MemorySemanticClass::try_from(value) {
        Ok(MemorySemanticClass::Structural) => "structural",
        Ok(MemorySemanticClass::Causal) => "causal",
        Ok(MemorySemanticClass::Motivational) => "motivational",
        Ok(MemorySemanticClass::Procedural) => "procedural",
        Ok(MemorySemanticClass::Evidential) => "evidential",
        Ok(MemorySemanticClass::Constraint) => "constraint",
        _ => "unspecified",
    }
}

fn confidence_label(value: i32) -> &'static str {
    match MemoryConfidence::try_from(value) {
        Ok(MemoryConfidence::High) => "high",
        Ok(MemoryConfidence::Medium) => "medium",
        Ok(MemoryConfidence::Low) => "low",
        Ok(MemoryConfidence::Unknown) => "unknown",
        _ => "unspecified",
    }
}

fn confidence_from_label(value: &str) -> i32 {
    match value {
        "high" => MemoryConfidence::High as i32,
        "medium" => MemoryConfidence::Medium as i32,
        "low" => MemoryConfidence::Low as i32,
        _ => MemoryConfidence::Unknown as i32,
    }
}

fn string_at(value: &Value, pointer: &str) -> String {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn strings_at(value: &Value, pointer: &str) -> Vec<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToString::to_string)
        .collect()
}

fn u64_at(value: &Value, pointer: &str) -> u64 {
    value
        .pointer(pointer)
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

fn u32_at(value: &Value, pointer: &str) -> u32 {
    u64_at(value, pointer).try_into().unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
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
        assert_eq!(compact["proof"]["path"], balanced["proof"]["path"]);
        assert_eq!(balanced["proof"]["path"], full["proof"]["path"]);
        for output in [&compact, &balanced, &full] {
            assert!(
                output["projection"]["page"]["returned"].as_u64() > Some(0),
                "a shortened core must still leave room for paging progress"
            );
        }
    }

    #[test]
    fn all_abouts_wake_with_a_shortened_core_advances_every_page() {
        let mut packet = large_fixture(30);
        packet
            .as_object_mut()
            .expect("wake packet")
            .remove("answer");
        packet
            .as_object_mut()
            .expect("wake packet")
            .remove("because");
        packet["wake"] = json!({
            "objective": "Sweep every memory anchor.",
            "current_state": [
                format!("Cross-project state: {}", "stable core detail ".repeat(240)),
                "Second project state",
                "Third project state"
            ],
            "causal_spine": [{
                "claim": "claim:0",
                "because": "The first evidence item anchors the sweep.",
                "evidence_ref": "evidence:0"
            }],
            "open_loops": ["Inspect every remaining anchor"],
            "next_actions": ["Continue with the returned cursor"],
            "guardrails": ["Never report a partial sweep as complete"]
        });
        let base_arguments = json!({
            "about": "project:kmp",
            "dimensions": {"scope": "all_abouts"},
            "budget": {"max_bytes": 4_000, "detail": "full"},
            "page": {"entries": 4}
        });

        let mut cursor = None;
        let mut expected_offset = 0_u64;
        let mut seen_cursors = BTreeSet::new();
        loop {
            let mut arguments = base_arguments.clone();
            if let Some(cursor) = &cursor {
                arguments["page"]["cursor"] = json!(cursor);
            }
            let output = projected(packet.clone(), arguments);
            let page = &output["projection"]["page"];
            let returned = page["returned"].as_u64().expect("returned");
            assert_eq!(page["offset"], expected_offset);
            assert!(
                returned > 0,
                "every continuation must make progress: {output}"
            );
            assert_eq!(output["projection"]["core_text_shortened"], true);
            assert!(
                serde_json::to_vec(&output).expect("projection bytes").len() <= 4_000,
                "the progress guarantee must preserve the byte ceiling"
            );
            expected_offset += returned;

            if page["has_more"] == false {
                assert!(page["next_cursor"].is_null());
                assert_eq!(page["total"], expected_offset);
                break;
            }
            let next = page["next_cursor"]
                .as_str()
                .expect("continuation cursor")
                .to_string();
            assert!(
                seen_cursors.insert(next.clone()),
                "a continuation cursor must never repeat"
            );
            cursor = Some(next);
            assert!(seen_cursors.len() < 100, "the fixture must terminate");
        }
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
    fn detail_levels_change_expansion_when_bytes_are_available() {
        let packet = large_fixture(80);
        let args = |detail: &str| {
            json!({
                "about": "project:kmp",
                "question": "What is current?",
                // Deliberately omit `tokens`: its default is a compatibility
                // hint, not a hidden cap on otherwise available bytes.
                "budget": {"max_bytes": 60_000, "detail": detail}
            })
        };
        let compact = projected(packet.clone(), args("compact"));
        let balanced = projected(packet.clone(), args("balanced"));
        let full = projected(packet, args("full"));

        let returned = |value: &Value| {
            value["projection"]["page"]["returned"]
                .as_u64()
                .expect("returned expansion count")
        };
        assert!(returned(&compact) < returned(&balanced));
        assert!(returned(&balanced) < returned(&full));
        let compact_relations = relation_set(&compact);
        let balanced_relations = relation_set(&balanced);
        let full_relations = relation_set(&full);
        assert!(compact_relations.is_subset(&balanced_relations));
        assert!(compact_relations.len() < balanced_relations.len());
        assert!(balanced_relations.is_subset(&full_relations));
        assert!(balanced_relations.len() < full_relations.len());
        assert!(
            compact["projection"]["excluded_by_detail"].as_u64()
                > balanced["projection"]["excluded_by_detail"].as_u64()
        );
        assert!(
            balanced["projection"]["excluded_by_detail"].as_u64()
                > full["projection"]["excluded_by_detail"].as_u64()
        );
        assert_eq!(full["projection"]["excluded_by_detail"], 0);
    }

    #[test]
    fn advisory_token_hint_does_not_filter_structured_content() {
        let packet = large_fixture(80);
        let args = |tokens: u32| {
            json!({
                "about": "project:kmp",
                "question": "What is current?",
                "budget": {"tokens": tokens, "max_bytes": 60_000, "detail": "full"}
            })
        };
        let tiny_hint = projected(packet.clone(), args(1));
        let large_hint = projected(packet, args(30_000));

        assert_eq!(tiny_hint["wake"], large_hint["wake"]);
        assert_eq!(tiny_hint["proof"], large_hint["proof"]);
        assert_eq!(
            tiny_hint["projection"]["page"]["returned"],
            large_hint["projection"]["page"]["returned"]
        );
        assert_eq!(tiny_hint["projection"]["budget"]["tokens_advisory"], 1);
        assert_eq!(
            large_hint["projection"]["budget"]["tokens_advisory"],
            30_000
        );
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
    fn cursor_identity_uses_the_canonical_ordered_selection() {
        let packet = large_fixture(12);
        let mut reordered = packet.clone();
        reordered["proof"]["path"]
            .as_array_mut()
            .expect("proof path")
            .reverse();
        let base_arguments = json!({
            "about": "project:kmp",
            "question": "Which storage engine is current?",
            "budget": {"tokens": 30_000, "max_bytes": 100_000, "detail": "full"},
            "page": {"entries": 4}
        });
        let first = projected(packet, base_arguments.clone());
        let cursor = first["projection"]["page"]["next_cursor"]
            .as_str()
            .expect("continuation cursor");
        let mut continuation = base_arguments;
        continuation["page"]["cursor"] = json!(cursor);

        let second = projected(reordered, continuation);
        assert_eq!(second["projection"]["page"]["offset"], 4);
        assert!(second["projection"]["page"]["returned"].as_u64() > Some(0));
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
    fn typed_ask_projection_round_trips_exact_bytes_and_cursor() {
        let response = typed_ask_fixture(24);
        let mut request = AskRequest {
            about: "project:kmp".to_string(),
            question: "Which storage engine is current?".to_string(),
            budget: Some(kmp_proto::v1beta1::MemoryBudget {
                tokens: 30_000,
                detail: MemoryDetailLevel::Full as i32,
                depth: 3,
                max_entries: 0,
                max_bytes: 4_000,
            }),
            page: Some(kmp_proto::v1beta1::PageRequest {
                entries: 4,
                cursor: String::new(),
            }),
            ..Default::default()
        };

        let first = project_ask_response(response.clone(), &request).expect("first typed page");
        let first_value = ask_value(&first);
        let first_bytes = serde_json::to_vec(&first_value).expect("serialized typed projection");
        assert!(first_bytes.len() <= 4_000);
        assert_eq!(
            first
                .projection
                .as_ref()
                .and_then(|projection| projection.budget.as_ref())
                .expect("typed projection budget")
                .used_bytes,
            first_bytes.len() as u64
        );
        let cursor = first
            .projection
            .as_ref()
            .and_then(|projection| projection.page.as_ref())
            .and_then(|page| page.next_cursor.clone())
            .expect("typed continuation cursor");

        request.page.as_mut().expect("page").cursor = cursor;
        let second = project_ask_response(response.clone(), &request).expect("second typed page");
        let second_page = second
            .projection
            .as_ref()
            .and_then(|projection| projection.page.as_ref())
            .expect("second page accounting");
        assert_eq!(second_page.offset, 4);
        assert!(second_page.returned > 0);

        request.question = "A changed question".to_string();
        let error = project_ask_response(response, &request)
            .expect_err("a typed cursor is bound to the recall selection");
        assert!(error.to_string().contains("does not match"));
    }

    #[test]
    fn typed_wake_projection_round_trips_exact_bytes_and_cursor() {
        let response = typed_wake_fixture(24);
        let mut request = WakeRequest {
            about: "project:kmp".to_string(),
            role: "implementer".to_string(),
            intent: "continue parity work".to_string(),
            budget: Some(kmp_proto::v1beta1::MemoryBudget {
                tokens: 30_000,
                detail: MemoryDetailLevel::Full as i32,
                depth: 3,
                max_entries: 0,
                max_bytes: 4_000,
            }),
            page: Some(kmp_proto::v1beta1::PageRequest {
                entries: 4,
                cursor: String::new(),
            }),
            ..Default::default()
        };

        let first = project_wake_response(response.clone(), &request).expect("first typed page");
        let first_value = wake_value(&first);
        let first_bytes = serde_json::to_vec(&first_value).expect("serialized typed projection");
        assert!(first_bytes.len() <= 4_000);
        assert_eq!(
            first
                .projection
                .as_ref()
                .and_then(|projection| projection.budget.as_ref())
                .expect("typed projection budget")
                .used_bytes,
            first_bytes.len() as u64
        );
        let cursor = first
            .projection
            .as_ref()
            .and_then(|projection| projection.page.as_ref())
            .and_then(|page| page.next_cursor.clone())
            .expect("typed continuation cursor");

        request.page.as_mut().expect("page").cursor = cursor;
        let second = project_wake_response(response, &request).expect("second typed page");
        let second_page = second
            .projection
            .as_ref()
            .and_then(|projection| projection.page.as_ref())
            .expect("second page accounting");
        assert_eq!(second_page.offset, 4);
        assert!(second_page.returned > 0);
    }

    #[test]
    fn shortened_wake_core_keeps_every_supersession_marker() {
        let mut response = typed_wake_fixture(24);
        response.proof.as_mut().expect("wake proof").superseded = (0..5)
            .map(|index| SupersededMemory {
                r#ref: format!("project:kmp:decision:old-{index}"),
                superseded_by: format!("project:kmp:decision:new-{index}"),
                why: format!(
                    "The later decision {index} replaces the earlier one because {}",
                    "the verified operating state changed ".repeat(24)
                ),
            })
            .collect();
        let request = WakeRequest {
            about: "project:kmp".to_string(),
            budget: Some(kmp_proto::v1beta1::MemoryBudget {
                tokens: 30_000,
                max_bytes: 4_000,
                detail: MemoryDetailLevel::Full as i32,
                depth: 1,
                max_entries: 0,
            }),
            ..Default::default()
        };

        let projected = project_wake_response(response, &request).expect("bounded wake");
        let proof = projected.proof.as_ref().expect("projected proof");
        assert_eq!(proof.superseded.len(), 5);
        for (index, marker) in proof.superseded.iter().enumerate() {
            assert_eq!(marker.r#ref, format!("project:kmp:decision:old-{index}"));
            assert_eq!(
                marker.superseded_by,
                format!("project:kmp:decision:new-{index}")
            );
            assert!(!marker.why.is_empty());
        }
        assert!(
            projected
                .projection
                .as_ref()
                .expect("projection")
                .core_text_shortened
        );
        assert!(
            serde_json::to_vec(&wake_value(&projected))
                .expect("serialized bounded wake")
                .len()
                <= 4_000
        );
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

    fn typed_ask_fixture(path_count: usize) -> AskResponse {
        let evidence = (0..8)
            .map(|index| MemoryEvidence {
                id: format!("evidence:{index}"),
                supports: vec![format!("claim:{index}")],
                text: format!(
                    "Canonical storage evidence {index}: {}",
                    "grounded detail ".repeat(8)
                ),
                source: format!("source:{index}"),
                time: None,
                metadata: Default::default(),
            })
            .collect::<Vec<_>>();
        let path = (0..path_count)
            .map(|index| {
                let (rel, semantic_class) = match index % 3 {
                    0 => ("depends_on", MemorySemanticClass::Causal),
                    1 => ("supports", MemorySemanticClass::Evidential),
                    _ => ("contains_entry", MemorySemanticClass::Structural),
                };
                MemoryRelation {
                    source_ref: format!("node:{index:04}"),
                    target_ref: format!("claim:{}", index % 3),
                    rel: rel.to_string(),
                    semantic_class: semantic_class as i32,
                    why: format!("Deterministic relation explanation {index}"),
                    evidence: String::new(),
                    confidence: MemoryConfidence::High as i32,
                    sequence: None,
                    explanation: None,
                    evidence_refs: Vec::new(),
                }
            })
            .collect();
        let because = (0..3)
            .map(|index| AnswerReason {
                claim: format!("claim:{index}"),
                evidence: String::new(),
                r#ref: format!("evidence:{index}"),
            })
            .collect();
        AskResponse {
            summary: "Deterministic memory answer from 3 evidence items.".to_string(),
            answer: "This legacy prose is normalized from typed citations.".to_string(),
            because,
            proof: Some(kmp_proto::v1beta1::Proof {
                path,
                evidence,
                conflicts: Vec::new(),
                missing: vec!["raw:one".to_string(), "raw:two".to_string()],
                confidence: MemoryConfidence::High as i32,
                superseded: Vec::new(),
                expired: Vec::new(),
                frontier_size: 2,
                matched_terms: vec!["storage".to_string(), "current".to_string()],
                matched_relations: vec!["supports".to_string()],
            }),
            warnings: Vec::new(),
            projection: None,
            truncation: None,
        }
    }

    fn typed_wake_fixture(path_count: usize) -> WakeResponse {
        let proof = typed_ask_fixture(path_count)
            .proof
            .expect("typed ask fixture proof");
        WakeResponse {
            summary: "Deterministic wake packet.".to_string(),
            wake: Some(kmp_proto::v1beta1::WakePacket {
                objective: "continue parity work".to_string(),
                current_state: (0..8)
                    .map(|index| format!("Current state {index}"))
                    .collect(),
                causal_spine: vec![WakeClaim {
                    claim: "claim:0".to_string(),
                    because: "The canonical evidence supports it.".to_string(),
                    evidence_ref: "evidence:0".to_string(),
                }],
                open_loops: (0..4).map(|index| format!("Open loop {index}")).collect(),
                next_actions: (0..4).map(|index| format!("Next action {index}")).collect(),
                guardrails: (0..4).map(|index| format!("Guardrail {index}")).collect(),
            }),
            proof: Some(proof),
            warnings: Vec::new(),
            resume_cursor: None,
            projection: None,
            truncation: None,
        }
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
