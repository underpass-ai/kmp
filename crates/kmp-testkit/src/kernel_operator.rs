use kmp_domain::{MemoryRelationQuality, MemoryRelationType, RelationSemanticClass};
use serde_json::{Map, Value};

pub fn kernel_operator_allowed_read_tools() -> Vec<String> {
    [
        "kmp_wake",
        "kmp_ask",
        "kmp_near",
        "kmp_goto",
        "kmp_rewind",
        "kmp_forward",
        "kmp_trace",
        "kmp_inspect",
    ]
    .iter()
    .map(ToString::to_string)
    .collect()
}

pub fn kernel_operator_allowed_write_tools() -> Vec<String> {
    ["kmp_ingest", "kmp_write_memory"]
        .iter()
        .map(ToString::to_string)
        .collect()
}

pub fn kernel_operator_allowed_full_tools() -> Vec<String> {
    let mut tools = kernel_operator_allowed_read_tools();
    tools.extend(kernel_operator_allowed_write_tools());
    tools
}

pub fn kernel_operator_is_bounded_tool_call(tool: &str, arguments: &Value) -> bool {
    match tool {
        "kmp_wake" => {
            path_non_empty_string(arguments, &["about"])
                && optional_limit(arguments, &["budget", "tokens"], 16_000)
                && optional_limit(arguments, &["budget", "depth"], 8)
                && optional_limit(arguments, &["depth"], 8)
        }
        "kmp_near" => {
            positive_limit(arguments, &["limit", "entries"], 64)
                && positive_limit(arguments, &["limit", "tokens"], 16_000)
                && optional_limit(arguments, &["budget", "tokens"], 16_000)
                && optional_limit(arguments, &["budget", "depth"], 8)
                && optional_limit(arguments, &["window", "before_entries"], 64)
                && optional_limit(arguments, &["window", "after_entries"], 64)
                && path_cursor(arguments, &["around"]).is_some()
        }
        "kmp_trace" => {
            path_non_empty_string(arguments, &["about"])
                && path_string(arguments, &["from"]).is_some()
                && path_string(arguments, &["to"]).is_some()
                && positive_limit(arguments, &["budget", "tokens"], 16_000)
                && optional_limit(arguments, &["budget", "depth"], 8)
                && optional_limit(arguments, &["page", "entries"], 256)
        }
        "kmp_inspect" => {
            path_non_empty_string(arguments, &["about"])
                && path_string(arguments, &["ref"]).is_some()
                && arguments
                    .pointer("/include/raw")
                    .and_then(Value::as_bool)
                    .is_some_and(|raw| !raw)
        }
        "kmp_goto" => {
            path_cursor(arguments, &["at"]).is_some()
                && optional_limit(arguments, &["limit", "entries"], 64)
                && optional_limit(arguments, &["limit", "tokens"], 16_000)
                && optional_limit(arguments, &["budget", "tokens"], 16_000)
        }
        "kmp_rewind" | "kmp_forward" => {
            path_cursor(arguments, &["from"]).is_some()
                && optional_limit(arguments, &["limit", "entries"], 64)
                && optional_limit(arguments, &["limit", "tokens"], 16_000)
                && optional_limit(arguments, &["budget", "tokens"], 16_000)
        }
        "kmp_ask" => optional_limit(arguments, &["budget", "tokens"], 16_000),
        "kmp_write_memory" => bounded_write_memory(arguments),
        "kmp_ingest" => bounded_ingest(arguments),
        _ => false,
    }
}

pub fn kernel_operator_action_shape_error(action: &Value) -> Option<String> {
    validate_action_shape(action).err()
}

pub fn kernel_operator_is_valid_action_shape(action: &Value) -> bool {
    kernel_operator_action_shape_error(action).is_none()
}

pub fn kernel_operator_action_contract_error(action: &Value) -> Option<String> {
    if let Some(error) = kernel_operator_action_shape_error(action) {
        return Some(error);
    }
    let (tool, arguments) = action_tool_arguments(action)?;
    if kernel_operator_is_bounded_tool_call(tool, arguments) {
        None
    } else {
        Some(format!("unbounded or invalid tool call for `{tool}`"))
    }
}

pub fn kernel_operator_primary_refs(action: &Value) -> Vec<String> {
    let Some(arguments) = action.get("arguments") else {
        return Vec::new();
    };
    let Some(tool) = action.get("tool").and_then(Value::as_str) else {
        return Vec::new();
    };
    match tool {
        "kmp_near" => path_string(arguments, &["around", "ref"])
            .map(|value| vec![value.to_string()])
            .unwrap_or_default(),
        "kmp_inspect" => path_string(arguments, &["ref"])
            .map(|value| vec![value.to_string()])
            .unwrap_or_default(),
        "kmp_trace" => {
            let mut refs = Vec::new();
            if let Some(from) = path_string(arguments, &["from"]) {
                refs.push(from.to_string());
            }
            if let Some(to) = path_string(arguments, &["to"]) {
                refs.push(to.to_string());
            }
            refs
        }
        "kmp_goto" => path_string(arguments, &["at", "ref"])
            .map(|value| vec![value.to_string()])
            .unwrap_or_default(),
        "kmp_rewind" | "kmp_forward" => path_string(arguments, &["from", "ref"])
            .map(|value| vec![value.to_string()])
            .unwrap_or_default(),
        "kmp_write_memory" => arguments
            .get("connect_to")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|link| link.get("ref").and_then(Value::as_str))
            .map(ToString::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn validate_action_shape(action: &Value) -> Result<(), String> {
    object(action, "action")?;
    let action_type = required_string(action, "type", "action")?;
    match action_type {
        "tool_call" => validate_tool_call_shape(action),
        "stop" => validate_stop_shape(action),
        other => Err(format!("unsupported action type `{other}`")),
    }
}

fn validate_tool_call_shape(action: &Value) -> Result<(), String> {
    exact_keys(action, "action", &["type", "tool", "arguments"], &[])?;
    let tool = required_string(action, "tool", "action")?;
    let arguments = required_value(action, "arguments", "action")?;
    object(arguments, "action.arguments")?;
    match tool {
        "kmp_wake" => validate_wake_arguments(arguments),
        "kmp_ask" => validate_ask_arguments(arguments),
        "kmp_near" => validate_temporal_arguments(arguments, "around", "kmp_near"),
        "kmp_goto" => validate_temporal_arguments(arguments, "at", "kmp_goto"),
        "kmp_rewind" => validate_temporal_arguments(arguments, "from", "kmp_rewind"),
        "kmp_forward" => validate_temporal_arguments(arguments, "from", "kmp_forward"),
        "kmp_trace" => validate_trace_arguments(arguments),
        "kmp_inspect" => validate_inspect_arguments(arguments),
        "kmp_write_memory" => validate_write_memory_arguments(arguments),
        "kmp_ingest" => validate_ingest_arguments(arguments),
        other => Err(format!("unsupported tool `{other}`")),
    }
}

fn validate_stop_shape(action: &Value) -> Result<(), String> {
    exact_keys(
        action,
        "action",
        &["type", "answer_policy", "final_refs", "reason"],
        &[],
    )?;
    validate_answer_policy(required_string(action, "answer_policy", "action")?)?;
    validate_string_array(
        required_value(action, "final_refs", "action")?,
        "action.final_refs",
    )?;
    required_non_empty_string(action, "reason", "action")?;
    Ok(())
}

fn validate_wake_arguments(arguments: &Value) -> Result<(), String> {
    exact_keys(
        arguments,
        "action.arguments",
        &["about"],
        &["role", "intent", "dimensions", "depth", "budget"],
    )?;
    required_non_empty_string(arguments, "about", "action.arguments")?;
    validate_optional_non_empty_string(arguments, "role", "action.arguments")?;
    validate_optional_non_empty_string(arguments, "intent", "action.arguments")?;
    if let Some(dimensions) = arguments.get("dimensions") {
        validate_dimensions(dimensions, "action.arguments.dimensions")?;
    }
    validate_optional_positive_integer(arguments, "depth", "action.arguments")?;
    if let Some(budget) = arguments.get("budget") {
        validate_budget(budget, "action.arguments.budget")?;
    }
    Ok(())
}

fn validate_ask_arguments(arguments: &Value) -> Result<(), String> {
    exact_keys(
        arguments,
        "action.arguments",
        &["about", "answer_policy", "dimensions", "question"],
        &["budget", "depth"],
    )?;
    required_non_empty_string(arguments, "about", "action.arguments")?;
    validate_answer_policy(required_string(
        arguments,
        "answer_policy",
        "action.arguments",
    )?)?;
    validate_dimensions(
        required_value(arguments, "dimensions", "action.arguments")?,
        "action.arguments.dimensions",
    )?;
    required_non_empty_string(arguments, "question", "action.arguments")?;
    if let Some(budget) = arguments.get("budget") {
        validate_budget(budget, "action.arguments.budget")?;
    }
    validate_optional_positive_integer(arguments, "depth", "action.arguments")?;
    Ok(())
}

fn validate_temporal_arguments(
    arguments: &Value,
    cursor_key: &str,
    tool: &str,
) -> Result<(), String> {
    exact_keys(
        arguments,
        "action.arguments",
        &[
            "about",
            cursor_key,
            "dimensions",
            "include",
            "limit",
            "budget",
            "window",
        ],
        &["depth"],
    )?;
    required_non_empty_string(arguments, "about", "action.arguments")?;
    validate_temporal_cursor(
        required_value(arguments, cursor_key, "action.arguments")?,
        &format!("action.arguments.{cursor_key}"),
    )?;
    validate_dimensions(
        required_value(arguments, "dimensions", "action.arguments")?,
        "action.arguments.dimensions",
    )?;
    validate_temporal_include(
        required_value(arguments, "include", "action.arguments")?,
        "action.arguments.include",
    )?;
    validate_limit(
        required_value(arguments, "limit", "action.arguments")?,
        "action.arguments.limit",
    )?;
    validate_budget(
        required_value(arguments, "budget", "action.arguments")?,
        "action.arguments.budget",
    )?;
    validate_window(
        required_value(arguments, "window", "action.arguments")?,
        "action.arguments.window",
    )?;
    validate_optional_positive_integer(arguments, "depth", "action.arguments")?;
    if !kernel_operator_allowed_read_tools()
        .iter()
        .any(|allowed| allowed == tool)
    {
        return Err(format!("unsupported tool `{tool}`"));
    }
    Ok(())
}

fn validate_trace_arguments(arguments: &Value) -> Result<(), String> {
    exact_keys(
        arguments,
        "action.arguments",
        &["about", "from", "to", "budget"],
        &["goal", "role", "page"],
    )?;
    required_non_empty_string(arguments, "about", "action.arguments")?;
    required_non_empty_string(arguments, "from", "action.arguments")?;
    required_non_empty_string(arguments, "to", "action.arguments")?;
    validate_budget(
        required_value(arguments, "budget", "action.arguments")?,
        "action.arguments.budget",
    )?;
    validate_optional_non_empty_string(arguments, "goal", "action.arguments")?;
    validate_optional_non_empty_string(arguments, "role", "action.arguments")?;
    if let Some(page) = arguments.get("page") {
        validate_page(page, "action.arguments.page")?;
    }
    Ok(())
}

fn validate_inspect_arguments(arguments: &Value) -> Result<(), String> {
    exact_keys(
        arguments,
        "action.arguments",
        &["about", "ref", "include"],
        &[],
    )?;
    required_non_empty_string(arguments, "about", "action.arguments")?;
    required_non_empty_string(arguments, "ref", "action.arguments")?;
    validate_inspect_include(
        required_value(arguments, "include", "action.arguments")?,
        "action.arguments.include",
    )?;
    Ok(())
}

fn validate_write_memory_arguments(arguments: &Value) -> Result<(), String> {
    exact_keys(
        arguments,
        "action.arguments",
        &[
            "about",
            "intent",
            "actor",
            "observed_at",
            "scope",
            "current",
            "connect_to",
            "read_context",
            "idempotency_key",
            "options",
        ],
        &["semantic_delta", "source_kind"],
    )?;
    required_non_empty_string(arguments, "about", "action.arguments")?;
    validate_writer_intent(required_string(arguments, "intent", "action.arguments")?)?;
    required_non_empty_string(arguments, "actor", "action.arguments")?;
    required_non_empty_string(arguments, "observed_at", "action.arguments")?;
    validate_write_scope(
        required_value(arguments, "scope", "action.arguments")?,
        "action.arguments.scope",
    )?;
    let current_ref = validate_write_current(
        required_value(arguments, "current", "action.arguments")?,
        "action.arguments.current",
    )?;
    let semantic_delta_ref = if let Some(delta) = arguments.get("semantic_delta") {
        validate_semantic_delta(delta, "action.arguments.semantic_delta")?
    } else {
        None
    };
    let read_context_refs = read_context_refs(
        required_value(arguments, "read_context", "action.arguments")?,
        "action.arguments.read_context",
    )?;
    let local_refs = [current_ref, semantic_delta_ref]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    validate_connect_to(
        required_value(arguments, "connect_to", "action.arguments")?,
        "action.arguments.connect_to",
        &local_refs,
        &read_context_refs,
    )?;
    required_non_empty_string(arguments, "idempotency_key", "action.arguments")?;
    validate_write_options(
        required_value(arguments, "options", "action.arguments")?,
        "action.arguments.options",
    )?;
    validate_optional_non_empty_string(arguments, "source_kind", "action.arguments")?;
    Ok(())
}

fn validate_ingest_arguments(arguments: &Value) -> Result<(), String> {
    exact_keys(
        arguments,
        "action.arguments",
        &[
            "about",
            "memory",
            "provenance",
            "idempotency_key",
            "dry_run",
        ],
        &[],
    )?;
    required_non_empty_string(arguments, "about", "action.arguments")?;
    validate_ingest_memory(
        required_value(arguments, "memory", "action.arguments")?,
        "action.arguments.memory",
    )?;
    validate_ingest_provenance(
        required_value(arguments, "provenance", "action.arguments")?,
        "action.arguments.provenance",
    )?;
    required_non_empty_string(arguments, "idempotency_key", "action.arguments")?;
    required_bool(arguments, "dry_run", "action.arguments")?;
    Ok(())
}

fn validate_dimensions(value: &Value, context: &str) -> Result<(), String> {
    exact_keys(
        value,
        context,
        &["mode", "scope"],
        &["include", "exclude", "scope_ids", "abouts"],
    )?;
    let mode = required_string(value, "mode", context)?;
    if !["all", "only", "except"].contains(&mode) {
        return Err(format!("{context}.mode has unsupported value `{mode}`"));
    }
    let scope = required_string(value, "scope", context)?;
    if !["current_about", "abouts", "all_abouts"].contains(&scope) {
        return Err(format!("{context}.scope has unsupported value `{scope}`"));
    }
    for field in ["include", "exclude", "scope_ids", "abouts"] {
        if let Some(values) = value.get(field) {
            validate_string_array(values, &format!("{context}.{field}"))?;
        }
    }
    let include_count = array_len(value.get("include"));
    let exclude_count = array_len(value.get("exclude"));
    let abouts_count = array_len(value.get("abouts"));
    match mode {
        "all" if include_count > 0 || exclude_count > 0 => {
            return Err(format!(
                "{context}.mode all must not set include or exclude values"
            ));
        }
        "only" if include_count == 0 => {
            return Err(format!("{context}.mode only requires include values"));
        }
        "only" if exclude_count > 0 => {
            return Err(format!("{context}.mode only must not set exclude values"));
        }
        "except" if exclude_count == 0 => {
            return Err(format!("{context}.mode except requires exclude values"));
        }
        "except" if include_count > 0 => {
            return Err(format!("{context}.mode except must not set include values"));
        }
        _ => {}
    }
    match scope {
        "current_about" if abouts_count > 0 => {
            return Err(format!("{context}.scope current_about must not set abouts"));
        }
        "abouts" if abouts_count == 0 => {
            return Err(format!(
                "{context}.scope abouts requires at least one about"
            ));
        }
        "all_abouts" if abouts_count > 0 => {
            return Err(format!("{context}.scope all_abouts must not set abouts"));
        }
        _ => {}
    }
    Ok(())
}

fn validate_temporal_cursor(value: &Value, context: &str) -> Result<(), String> {
    exact_keys(value, context, &[], &["ref", "time", "sequence"])?;
    let object = object(value, context)?;
    let selected = ["ref", "time", "sequence"]
        .iter()
        .filter(|field| object.contains_key(**field))
        .count();
    if selected != 1 {
        return Err(format!(
            "{context} must set exactly one of ref, time, or sequence"
        ));
    }
    if value.get("ref").is_some() {
        required_non_empty_string(value, "ref", context)?;
    }
    if value.get("time").is_some() {
        required_non_empty_string(value, "time", context)?;
    }
    if value.get("sequence").is_some() {
        required_positive_integer(value, "sequence", context)?;
    }
    Ok(())
}

fn validate_temporal_include(value: &Value, context: &str) -> Result<(), String> {
    exact_keys(value, context, &["evidence", "raw_refs", "relations"], &[])?;
    required_bool(value, "evidence", context)?;
    required_bool(value, "raw_refs", context)?;
    required_bool(value, "relations", context)?;
    Ok(())
}

fn validate_inspect_include(value: &Value, context: &str) -> Result<(), String> {
    exact_keys(
        value,
        context,
        &["details", "incoming", "outgoing", "raw"],
        &[],
    )?;
    required_bool(value, "details", context)?;
    required_bool(value, "incoming", context)?;
    required_bool(value, "outgoing", context)?;
    let raw = required_bool(value, "raw", context)?;
    if raw {
        return Err(format!("{context}.raw must be false"));
    }
    Ok(())
}

fn validate_limit(value: &Value, context: &str) -> Result<(), String> {
    exact_keys(value, context, &["entries", "tokens"], &[])?;
    required_positive_integer(value, "entries", context)?;
    required_positive_integer(value, "tokens", context)?;
    Ok(())
}

fn validate_budget(value: &Value, context: &str) -> Result<(), String> {
    exact_keys(value, context, &[], &["tokens", "depth", "detail"])?;
    let object = object(value, context)?;
    if object.is_empty() {
        return Err(format!("{context} must not be empty"));
    }
    validate_optional_positive_integer(value, "tokens", context)?;
    validate_optional_positive_integer(value, "depth", context)?;
    if let Some(detail) = value.get("detail") {
        let Some(detail) = detail.as_str() else {
            return Err(format!("{context}.detail must be a string"));
        };
        if !["compact", "balanced", "full"].contains(&detail) {
            return Err(format!("{context}.detail has unsupported value `{detail}`"));
        }
    }
    Ok(())
}

fn validate_window(value: &Value, context: &str) -> Result<(), String> {
    exact_keys(value, context, &["before_entries", "after_entries"], &[])?;
    required_u64(value, "before_entries", context)?;
    required_u64(value, "after_entries", context)?;
    Ok(())
}

fn validate_page(value: &Value, context: &str) -> Result<(), String> {
    exact_keys(value, context, &[], &["entries", "cursor"])?;
    validate_optional_positive_integer(value, "entries", context)?;
    validate_optional_non_empty_string(value, "cursor", context)?;
    Ok(())
}

fn validate_write_scope(value: &Value, context: &str) -> Result<(), String> {
    exact_keys(value, context, &["process"], &["task", "episode"])?;
    required_non_empty_string(value, "process", context)?;
    validate_optional_non_empty_string(value, "task", context)?;
    validate_optional_non_empty_string(value, "episode", context)?;
    Ok(())
}

fn validate_write_current(value: &Value, context: &str) -> Result<Option<String>, String> {
    exact_keys(value, context, &["kind", "summary", "evidence"], &["ref"])?;
    validate_writer_node_kind(required_string(value, "kind", context)?)?;
    required_non_empty_string(value, "summary", context)?;
    required_non_empty_string(value, "evidence", context)?;
    validate_optional_non_empty_string(value, "ref", context)?;
    Ok(value
        .get("ref")
        .and_then(Value::as_str)
        .map(ToString::to_string))
}

fn validate_semantic_delta(value: &Value, context: &str) -> Result<Option<String>, String> {
    exact_keys(value, context, &["from", "to", "why", "evidence"], &["ref"])?;
    for field in ["from", "to", "why", "evidence"] {
        required_non_empty_string(value, field, context)?;
    }
    validate_optional_non_empty_string(value, "ref", context)?;
    Ok(value
        .get("ref")
        .and_then(Value::as_str)
        .map(ToString::to_string))
}

fn validate_connect_to(
    value: &Value,
    context: &str,
    local_refs: &[String],
    read_context_refs: &[String],
) -> Result<(), String> {
    let links = non_empty_array(value, context)?;
    for (index, link) in links.iter().enumerate() {
        let link_context = format!("{context}[{index}]");
        exact_keys(
            link,
            &link_context,
            &["ref", "rel", "class", "why", "evidence"],
            &["confidence"],
        )?;
        let target_ref = required_non_empty_string(link, "ref", &link_context)?;
        let rel = required_non_empty_string(link, "rel", &link_context)?;
        let class = required_non_empty_string(link, "class", &link_context)?;
        let semantic_class = RelationSemanticClass::parse(class)
            .map_err(|error| format!("{link_context}.class is invalid: {error}"))?;
        let relation_type = MemoryRelationType::new(rel)
            .map_err(|error| format!("{link_context}.rel is invalid: {error}"))?;
        let Some(spec) = relation_type.writer_spec() else {
            return Err(format!("{link_context}.rel is outside writer vocabulary"));
        };
        if !spec.allows_class(&semantic_class) {
            return Err(format!(
                "{link_context}.class `{class}` is not allowed for relation `{rel}`"
            ));
        }
        if semantic_class != RelationSemanticClass::Structural {
            required_non_empty_string(link, "why", &link_context)?;
            required_non_empty_string(link, "evidence", &link_context)?;
        }
        if let Some(confidence) = link.get("confidence").and_then(Value::as_str) {
            validate_confidence(confidence, &format!("{link_context}.confidence"))?;
        }
        let target_is_local = local_refs.iter().any(|local_ref| local_ref == target_ref);
        let target_was_read = read_context_refs
            .iter()
            .any(|read_ref| read_ref == target_ref);
        if spec.quality() == MemoryRelationQuality::Rich && !target_is_local && !target_was_read {
            return Err(format!(
                "{link_context}.ref `{target_ref}` uses a rich relation without read_context proof"
            ));
        }
    }
    Ok(())
}

fn read_context_refs(value: &Value, context: &str) -> Result<Vec<String>, String> {
    exact_keys(
        value,
        context,
        &[],
        &[
            "inspected_refs",
            "temporal_refs",
            "wake_refs",
            "ask_refs",
            "trace_paths",
        ],
    )?;
    let mut refs = Vec::new();
    for field in ["inspected_refs", "temporal_refs", "wake_refs", "ask_refs"] {
        if let Some(values) = value.get(field) {
            for item in string_array(values, &format!("{context}.{field}"))? {
                refs.push(item.to_string());
            }
        }
    }
    if let Some(paths) = value.get("trace_paths") {
        for (index, path) in array(paths, &format!("{context}.trace_paths"))?
            .iter()
            .enumerate()
        {
            let path_context = format!("{context}.trace_paths[{index}]");
            exact_keys(path, &path_context, &["from", "to"], &["refs"])?;
            refs.push(required_non_empty_string(path, "from", &path_context)?.to_string());
            refs.push(required_non_empty_string(path, "to", &path_context)?.to_string());
            if let Some(path_refs) = path.get("refs") {
                for item in string_array(path_refs, &format!("{path_context}.refs"))? {
                    refs.push(item.to_string());
                }
            }
        }
    }
    Ok(refs)
}

fn validate_write_options(value: &Value, context: &str) -> Result<(), String> {
    exact_keys(value, context, &["dry_run", "strict"], &["sequence"])?;
    required_bool(value, "dry_run", context)?;
    let strict = required_bool(value, "strict", context)?;
    if !strict {
        return Err(format!("{context}.strict must be true for operator-write"));
    }
    validate_optional_positive_integer(value, "sequence", context)?;
    Ok(())
}

fn validate_ingest_memory(value: &Value, context: &str) -> Result<(), String> {
    exact_keys(
        value,
        context,
        &["dimensions", "entries", "relations", "evidence"],
        &[],
    )?;
    validate_dimensions_payload(required_value(value, "dimensions", context)?, context)?;
    validate_entries_payload(required_value(value, "entries", context)?, context)?;
    validate_relations_payload(required_value(value, "relations", context)?, context)?;
    validate_evidence_payload(required_value(value, "evidence", context)?, context)?;
    Ok(())
}

fn validate_dimensions_payload(value: &Value, context: &str) -> Result<(), String> {
    for (index, dimension) in non_empty_array(value, &format!("{context}.dimensions"))?
        .iter()
        .enumerate()
    {
        let dimension_context = format!("{context}.dimensions[{index}]");
        exact_keys(dimension, &dimension_context, &["id", "kind"], &["title"])?;
        required_non_empty_string(dimension, "id", &dimension_context)?;
        required_non_empty_string(dimension, "kind", &dimension_context)?;
        validate_optional_non_empty_string(dimension, "title", &dimension_context)?;
    }
    Ok(())
}

fn validate_entries_payload(value: &Value, context: &str) -> Result<(), String> {
    for (index, entry) in non_empty_array(value, &format!("{context}.entries"))?
        .iter()
        .enumerate()
    {
        let entry_context = format!("{context}.entries[{index}]");
        exact_keys(
            entry,
            &entry_context,
            &["id", "kind", "text"],
            &["coordinates", "metadata"],
        )?;
        required_non_empty_string(entry, "id", &entry_context)?;
        required_non_empty_string(entry, "kind", &entry_context)?;
        required_non_empty_string(entry, "text", &entry_context)?;
        if let Some(coordinates) = entry.get("coordinates") {
            validate_coordinates(coordinates, &format!("{entry_context}.coordinates"))?;
        }
    }
    Ok(())
}

fn validate_coordinates(value: &Value, context: &str) -> Result<(), String> {
    for (index, coordinate) in array(value, context)?.iter().enumerate() {
        let coordinate_context = format!("{context}[{index}]");
        exact_keys(
            coordinate,
            &coordinate_context,
            &["dimension", "scope_id"],
            &["sequence", "observed_at", "occurred_at", "valid_from"],
        )?;
        required_non_empty_string(coordinate, "dimension", &coordinate_context)?;
        required_non_empty_string(coordinate, "scope_id", &coordinate_context)?;
        validate_optional_positive_integer(coordinate, "sequence", &coordinate_context)?;
        for field in ["observed_at", "occurred_at", "valid_from"] {
            validate_optional_non_empty_string(coordinate, field, &coordinate_context)?;
        }
    }
    Ok(())
}

fn validate_relations_payload(value: &Value, context: &str) -> Result<(), String> {
    for (index, relation) in array(value, &format!("{context}.relations"))?
        .iter()
        .enumerate()
    {
        let relation_context = format!("{context}.relations[{index}]");
        exact_keys(
            relation,
            &relation_context,
            &["from", "to", "rel", "class", "why", "evidence"],
            &["confidence", "sequence"],
        )?;
        required_non_empty_string(relation, "from", &relation_context)?;
        required_non_empty_string(relation, "to", &relation_context)?;
        let rel = required_non_empty_string(relation, "rel", &relation_context)?;
        MemoryRelationType::new(rel)
            .map_err(|error| format!("{relation_context}.rel is invalid: {error}"))?;
        let class = required_non_empty_string(relation, "class", &relation_context)?;
        RelationSemanticClass::parse(class)
            .map_err(|error| format!("{relation_context}.class is invalid: {error}"))?;
        required_non_empty_string(relation, "why", &relation_context)?;
        required_non_empty_string(relation, "evidence", &relation_context)?;
        if let Some(confidence) = relation.get("confidence").and_then(Value::as_str) {
            validate_confidence(confidence, &format!("{relation_context}.confidence"))?;
        }
        validate_optional_positive_integer(relation, "sequence", &relation_context)?;
    }
    Ok(())
}

fn validate_evidence_payload(value: &Value, context: &str) -> Result<(), String> {
    for (index, evidence) in array(value, &format!("{context}.evidence"))?
        .iter()
        .enumerate()
    {
        let evidence_context = format!("{context}.evidence[{index}]");
        exact_keys(
            evidence,
            &evidence_context,
            &["id", "supports", "text"],
            &["source", "time"],
        )?;
        required_non_empty_string(evidence, "id", &evidence_context)?;
        validate_string_array(
            required_value(evidence, "supports", &evidence_context)?,
            &format!("{evidence_context}.supports"),
        )?;
        required_non_empty_string(evidence, "text", &evidence_context)?;
        validate_optional_non_empty_string(evidence, "source", &evidence_context)?;
        validate_optional_non_empty_string(evidence, "time", &evidence_context)?;
    }
    Ok(())
}

fn validate_ingest_provenance(value: &Value, context: &str) -> Result<(), String> {
    exact_keys(
        value,
        context,
        &[
            "source_kind",
            "source_agent",
            "observed_at",
            "correlation_id",
            "causation_id",
        ],
        &[],
    )?;
    for field in [
        "source_kind",
        "source_agent",
        "observed_at",
        "correlation_id",
        "causation_id",
    ] {
        required_non_empty_string(value, field, context)?;
    }
    Ok(())
}

fn validate_answer_policy(value: &str) -> Result<(), String> {
    if ["evidence_or_unknown", "show_conflicts", "best_effort"].contains(&value) {
        Ok(())
    } else {
        Err(format!("unsupported answer_policy `{value}`"))
    }
}

fn validate_writer_intent(value: &str) -> Result<(), String> {
    if [
        "record_turn",
        "record_observation",
        "record_decision",
        "record_feedback",
        "record_delta",
    ]
    .contains(&value)
    {
        Ok(())
    } else {
        Err(format!("unsupported writer intent `{value}`"))
    }
}

fn validate_writer_node_kind(value: &str) -> Result<(), String> {
    if [
        "turn",
        "observation",
        "decision",
        "feedback",
        "semantic_delta",
        "constraint",
        "preference",
        "derived_value",
        "error_path",
        "success_path",
    ]
    .contains(&value)
    {
        Ok(())
    } else {
        Err(format!("unsupported writer current.kind `{value}`"))
    }
}

fn validate_confidence(value: &str, context: &str) -> Result<(), String> {
    if ["high", "medium", "low", "unknown"].contains(&value) {
        Ok(())
    } else {
        Err(format!("{context} has unsupported value `{value}`"))
    }
}

fn action_tool_arguments(action: &Value) -> Option<(&str, &Value)> {
    if action.get("type").and_then(Value::as_str) != Some("tool_call") {
        return None;
    }
    let tool = action.get("tool").and_then(Value::as_str)?;
    let arguments = action.get("arguments")?;
    Some((tool, arguments))
}

fn exact_keys<'a>(
    value: &'a Value,
    context: &str,
    required: &[&str],
    optional: &[&str],
) -> Result<&'a Map<String, Value>, String> {
    let object = object(value, context)?;
    for key in required {
        if !object.contains_key(*key) {
            return Err(format!("{context} missing required field `{key}`"));
        }
    }
    for key in object.keys() {
        if !required.contains(&key.as_str()) && !optional.contains(&key.as_str()) {
            return Err(format!("{context} has unexpected field `{key}`"));
        }
    }
    Ok(object)
}

fn object<'a>(value: &'a Value, context: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{context} must be an object"))
}

fn required_value<'a>(value: &'a Value, field: &str, context: &str) -> Result<&'a Value, String> {
    value
        .get(field)
        .ok_or_else(|| format!("{context} missing required field `{field}`"))
}

fn required_string<'a>(value: &'a Value, field: &str, context: &str) -> Result<&'a str, String> {
    required_value(value, field, context)?
        .as_str()
        .ok_or_else(|| format!("{context}.{field} must be a string"))
}

fn required_non_empty_string<'a>(
    value: &'a Value,
    field: &str,
    context: &str,
) -> Result<&'a str, String> {
    let value = required_string(value, field, context)?;
    if value.is_empty() {
        Err(format!("{context}.{field} must not be empty"))
    } else {
        Ok(value)
    }
}

fn validate_optional_non_empty_string(
    value: &Value,
    field: &str,
    context: &str,
) -> Result<(), String> {
    if value.get(field).is_some() {
        required_non_empty_string(value, field, context)?;
    }
    Ok(())
}

fn required_bool(value: &Value, field: &str, context: &str) -> Result<bool, String> {
    required_value(value, field, context)?
        .as_bool()
        .ok_or_else(|| format!("{context}.{field} must be a boolean"))
}

fn required_positive_integer(value: &Value, field: &str, context: &str) -> Result<u64, String> {
    let actual = required_u64(value, field, context)?;
    if actual == 0 {
        Err(format!("{context}.{field} must be > 0"))
    } else {
        Ok(actual)
    }
}

fn required_u64(value: &Value, field: &str, context: &str) -> Result<u64, String> {
    required_value(value, field, context)?
        .as_u64()
        .ok_or_else(|| format!("{context}.{field} must be a non-negative integer"))
}

fn validate_optional_positive_integer(
    value: &Value,
    field: &str,
    context: &str,
) -> Result<(), String> {
    if value.get(field).is_some() {
        required_positive_integer(value, field, context)?;
    }
    Ok(())
}

fn validate_string_array(value: &Value, context: &str) -> Result<(), String> {
    let Some(values) = value.as_array() else {
        return Err(format!("{context} must be an array"));
    };
    for (index, value) in values.iter().enumerate() {
        let Some(item) = value.as_str() else {
            return Err(format!("{context}[{index}] must be a string"));
        };
        if item.is_empty() {
            return Err(format!("{context}[{index}] must not be empty"));
        }
    }
    Ok(())
}

fn array<'a>(value: &'a Value, context: &str) -> Result<&'a [Value], String> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| format!("{context} must be an array"))
}

fn non_empty_array<'a>(value: &'a Value, context: &str) -> Result<&'a [Value], String> {
    let values = array(value, context)?;
    if values.is_empty() {
        Err(format!("{context} must not be empty"))
    } else {
        Ok(values)
    }
}

fn string_array<'a>(value: &'a Value, context: &str) -> Result<Vec<&'a str>, String> {
    validate_string_array(value, context)?;
    Ok(value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect())
}

fn array_len(value: Option<&Value>) -> usize {
    value.and_then(Value::as_array).map(Vec::len).unwrap_or(0)
}

fn positive_limit(value: &Value, path: &[&str], max: u64) -> bool {
    path_u64(value, path).is_some_and(|actual| actual > 0 && actual <= max)
}

fn optional_limit(value: &Value, path: &[&str], max: u64) -> bool {
    path_u64(value, path).is_none_or(|actual| actual <= max)
}

fn bounded_write_memory(arguments: &Value) -> bool {
    validate_write_memory_arguments(arguments).is_ok()
        && arguments
            .pointer("/options/strict")
            .and_then(Value::as_bool)
            == Some(true)
        && arguments
            .pointer("/options/dry_run")
            .and_then(Value::as_bool)
            .is_some()
        && optional_limit(arguments, &["options", "sequence"], u32::MAX.into())
        && arguments
            .pointer("/connect_to")
            .and_then(Value::as_array)
            .is_some_and(|links| !links.is_empty() && links.len() <= 32)
}

fn bounded_ingest(arguments: &Value) -> bool {
    validate_ingest_arguments(arguments).is_ok()
        && arguments.get("dry_run").and_then(Value::as_bool).is_some()
        && arguments
            .pointer("/memory/dimensions")
            .and_then(Value::as_array)
            .is_some_and(|values| !values.is_empty() && values.len() <= 64)
        && arguments
            .pointer("/memory/entries")
            .and_then(Value::as_array)
            .is_some_and(|values| !values.is_empty() && values.len() <= 256)
        && arguments
            .pointer("/memory/relations")
            .and_then(Value::as_array)
            .is_some_and(|values| values.len() <= 512)
        && arguments
            .pointer("/memory/evidence")
            .and_then(Value::as_array)
            .is_some_and(|values| values.len() <= 512)
}

fn path_u64(value: &Value, path: &[&str]) -> Option<u64> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_u64()
}

fn path_string<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str()
}

fn path_non_empty_string(value: &Value, path: &[&str]) -> bool {
    path_string(value, path).is_some_and(|actual| !actual.is_empty())
}

fn path_cursor<'a>(value: &'a Value, path: &[&str]) -> Option<(&'static str, &'a Value)> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    let object = current.as_object()?;
    let mut selected = None;
    for key in ["ref", "time", "sequence"] {
        if let Some(value) = object.get(key) {
            if selected.is_some() {
                return None;
            }
            selected = Some((key, value));
        }
    }
    match selected {
        Some(("ref" | "time", Value::String(value))) if !value.is_empty() => selected,
        Some(("sequence", value)) if value.as_u64().is_some_and(|actual| actual > 0) => selected,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        kernel_operator_action_contract_error, kernel_operator_action_shape_error,
        kernel_operator_is_bounded_tool_call, kernel_operator_primary_refs,
    };

    #[test]
    fn bounded_tool_detection_accepts_expected_navigation_calls() {
        assert!(kernel_operator_is_bounded_tool_call(
            "kmp_near",
            &json!({
                "around": { "time": "2026-05-14T00:00:00Z" },
                "limit": { "entries": 12, "tokens": 2400 },
                "budget": { "depth": 3, "tokens": 2400 },
                "window": { "before_entries": 6, "after_entries": 0 }
            })
        ));
        assert!(kernel_operator_is_bounded_tool_call(
            "kmp_trace",
            &json!({
                "about": "about:1",
                "from": "node:2",
                "to": "node:1",
                "budget": { "depth": 1, "tokens": 1600 }
            })
        ));
        assert!(kernel_operator_is_bounded_tool_call(
            "kmp_inspect",
            &json!({
                "about": "about:1",
                "ref": "node:1",
                "include": { "details": true, "incoming": true, "outgoing": true, "raw": false }
            })
        ));
    }

    #[test]
    fn bounded_tool_detection_rejects_unbounded_calls() {
        assert!(!kernel_operator_is_bounded_tool_call(
            "kmp_near",
            &json!({
                "around": { "ref": "node:1" },
                "limit": { "entries": 500, "tokens": 2400 }
            })
        ));
        assert!(!kernel_operator_is_bounded_tool_call(
            "kmp_inspect",
            &json!({
                "ref": "node:1",
                "include": { "raw": true }
            })
        ));
    }

    #[test]
    fn primary_refs_extracts_tool_ref_shape() {
        assert_eq!(
            kernel_operator_primary_refs(&json!({
                "type": "tool_call",
                "tool": "kmp_trace",
                "arguments": {
                    "from": "node:2",
                    "to": "node:1"
                }
            })),
            ["node:2".to_string(), "node:1".to_string()]
        );
        assert_eq!(
            kernel_operator_primary_refs(&json!({
                "type": "tool_call",
                "tool": "kmp_write_memory",
                "arguments": {
                    "connect_to": [
                        { "ref": "node:prior", "rel": "chosen_because" },
                        { "ref": "node:fallback", "rel": "follows" }
                    ]
                }
            })),
            ["node:prior".to_string(), "node:fallback".to_string()]
        );
    }

    #[test]
    fn action_shape_accepts_expected_operator_calls() {
        for action in [
            json!({
                "type": "tool_call",
                "tool": "kmp_wake",
                "arguments": {
                    "about": "about:1",
                    "intent": "continue investigation",
                    "dimensions": { "mode": "only", "include": ["agent"], "scope": "abouts", "abouts": ["about:2"] },
                    "budget": { "depth": 2, "tokens": 2400 }
                }
            }),
            json!({
                "type": "tool_call",
                "tool": "kmp_near",
                "arguments": {
                    "about": "about:1",
                    "around": { "sequence": 7 },
                    "dimensions": { "mode": "except", "exclude": ["discarded"], "scope": "all_abouts" },
                    "include": { "evidence": true, "raw_refs": false, "relations": true },
                    "limit": { "entries": 12, "tokens": 2400 },
                    "budget": { "depth": 3, "tokens": 2400 },
                    "window": { "before_entries": 6, "after_entries": 0 }
                }
            }),
            json!({
                "type": "tool_call",
                "tool": "kmp_inspect",
                "arguments": {
                    "about": "about:1",
                    "ref": "node:1",
                    "include": {
                        "details": true,
                        "incoming": true,
                        "outgoing": true,
                        "raw": false
                    }
                }
            }),
            json!({
                "type": "tool_call",
                "tool": "kmp_ask",
                "arguments": {
                    "about": "about:1",
                    "answer_policy": "evidence_or_unknown",
                    "dimensions": { "mode": "all", "scope": "current_about" },
                    "question": "What changed?"
                }
            }),
            json!({
                "type": "stop",
                "answer_policy": "evidence_or_unknown",
                "final_refs": ["node:1"],
                "reason": "sufficient_evidence"
            }),
        ] {
            assert_eq!(kernel_operator_action_shape_error(&action), None);
        }
    }

    #[test]
    fn action_shape_rejects_invalid_dimension_semantics() {
        for (dimensions, expected) in [
            (
                json!({ "mode": "all", "scope": "current_about", "include": ["agent"] }),
                "action.arguments.dimensions.mode all must not set include or exclude values",
            ),
            (
                json!({ "mode": "only", "scope": "abouts", "abouts": ["about:2"] }),
                "action.arguments.dimensions.mode only requires include values",
            ),
            (
                json!({ "mode": "only", "scope": "current_about", "include": ["agent"], "exclude": ["discarded"] }),
                "action.arguments.dimensions.mode only must not set exclude values",
            ),
            (
                json!({ "mode": "except", "scope": "current_about" }),
                "action.arguments.dimensions.mode except requires exclude values",
            ),
            (
                json!({ "mode": "except", "scope": "current_about", "include": ["agent"], "exclude": ["discarded"] }),
                "action.arguments.dimensions.mode except must not set include values",
            ),
            (
                json!({ "mode": "all", "scope": "current_about", "abouts": ["about:2"] }),
                "action.arguments.dimensions.scope current_about must not set abouts",
            ),
            (
                json!({ "mode": "all", "scope": "abouts" }),
                "action.arguments.dimensions.scope abouts requires at least one about",
            ),
            (
                json!({ "mode": "all", "scope": "all_abouts", "abouts": ["about:2"] }),
                "action.arguments.dimensions.scope all_abouts must not set abouts",
            ),
        ] {
            let action = json!({
                "type": "tool_call",
                "tool": "kmp_ask",
                "arguments": {
                    "about": "about:1",
                    "answer_policy": "evidence_or_unknown",
                    "dimensions": dimensions,
                    "question": "What changed?"
                }
            });

            assert_eq!(
                kernel_operator_action_shape_error(&action),
                Some(expected.to_string())
            );
        }
    }

    #[test]
    fn action_shape_rejects_ambiguous_temporal_cursor() {
        let action = json!({
            "type": "tool_call",
            "tool": "kmp_near",
            "arguments": {
                "about": "about:1",
                "around": { "ref": "node:1", "sequence": 1 },
                "dimensions": { "mode": "all", "scope": "current_about" },
                "include": { "evidence": true, "raw_refs": false, "relations": true },
                "limit": { "entries": 12, "tokens": 2400 },
                "budget": { "depth": 3, "tokens": 2400 },
                "window": { "before_entries": 6, "after_entries": 0 }
            }
        });

        assert_eq!(
            kernel_operator_action_shape_error(&action),
            Some(
                "action.arguments.around must set exactly one of ref, time, or sequence"
                    .to_string()
            )
        );
    }

    #[test]
    fn action_contract_accepts_all_temporal_cursor_modes() {
        for cursor in [
            json!({ "ref": "node:1" }),
            json!({ "time": "2026-05-14T00:00:00Z" }),
            json!({ "sequence": 7 }),
        ] {
            let action = json!({
                "type": "tool_call",
                "tool": "kmp_near",
                "arguments": {
                    "about": "about:1",
                    "around": cursor,
                    "dimensions": { "mode": "all", "scope": "current_about" },
                    "include": { "evidence": true, "raw_refs": false, "relations": true },
                    "limit": { "entries": 12, "tokens": 2400 },
                    "budget": { "depth": 3, "tokens": 2400 },
                    "window": { "before_entries": 6, "after_entries": 0 }
                }
            });

            assert_eq!(kernel_operator_action_contract_error(&action), None);
        }
    }

    #[test]
    fn action_shape_rejects_extra_tool_argument_fields() {
        let action = json!({
            "type": "tool_call",
            "tool": "kmp_ask",
            "arguments": {
                "about": "about:1",
                "answer_policy": "evidence_or_unknown",
                "dimensions": { "mode": "all", "scope": "current_about" },
                "question": "What changed?",
                "final_refs": ["node:1"]
            }
        });

        assert_eq!(
            kernel_operator_action_shape_error(&action),
            Some("action.arguments has unexpected field `final_refs`".to_string())
        );
    }

    #[test]
    fn action_shape_rejects_extra_top_level_fields() {
        let action = json!({
            "type": "tool_call",
            "tool": "kmp_inspect",
            "arguments": {
                "ref": "node:1",
                "include": {
                    "details": true,
                    "incoming": true,
                    "outgoing": true,
                    "raw": false
                }
            },
            "confidence": "high"
        });

        assert_eq!(
            kernel_operator_action_shape_error(&action),
            Some("action has unexpected field `confidence`".to_string())
        );
    }

    #[test]
    fn action_contract_rejects_unbounded_navigation() {
        let action = json!({
            "type": "tool_call",
            "tool": "kmp_near",
            "arguments": {
                "about": "about:1",
                "around": { "ref": "node:1" },
                "dimensions": { "mode": "all", "scope": "current_about" },
                "include": { "evidence": true, "raw_refs": false, "relations": true },
                "limit": { "entries": 1000, "tokens": 2400 },
                "budget": { "depth": 3, "tokens": 2400 },
                "window": { "before_entries": 6, "after_entries": 0 }
            }
        });

        assert_eq!(
            kernel_operator_action_contract_error(&action),
            Some("unbounded or invalid tool call for `kmp_near`".to_string())
        );
    }

    #[test]
    fn action_contract_accepts_smart_write_memory() {
        let action = json!({
            "type": "tool_call",
            "tool": "kmp_write_memory",
            "arguments": valid_write_memory_arguments()
        });

        assert_eq!(kernel_operator_action_contract_error(&action), None);
    }

    #[test]
    fn action_contract_rejects_rich_write_without_read_context_proof() {
        let mut arguments = valid_write_memory_arguments();
        arguments["read_context"] = json!({
            "inspected_refs": []
        });
        let action = json!({
            "type": "tool_call",
            "tool": "kmp_write_memory",
            "arguments": arguments
        });

        assert_eq!(
            kernel_operator_action_contract_error(&action),
            Some(
                "action.arguments.connect_to[0].ref `incident:mobile-login:observation:401-refresh-race` uses a rich relation without read_context proof"
                    .to_string()
            )
        );
    }

    #[test]
    fn action_contract_rejects_smart_write_without_relation_evidence() {
        let mut arguments = valid_write_memory_arguments();
        arguments["connect_to"][0]["evidence"] = json!("");
        let action = json!({
            "type": "tool_call",
            "tool": "kmp_write_memory",
            "arguments": arguments
        });

        assert_eq!(
            kernel_operator_action_contract_error(&action),
            Some("action.arguments.connect_to[0].evidence must not be empty".to_string())
        );
    }

    #[test]
    fn action_contract_accepts_canonical_ingest_write() {
        let action = json!({
            "type": "tool_call",
            "tool": "kmp_ingest",
            "arguments": valid_ingest_arguments()
        });

        assert_eq!(kernel_operator_action_contract_error(&action), None);
    }

    fn valid_write_memory_arguments() -> serde_json::Value {
        json!({
            "about": "incident:mobile-login",
            "intent": "record_decision",
            "actor": "agent:backend",
            "observed_at": "2026-05-06T10:00:00Z",
            "scope": {
                "task": "incident:mobile-login",
                "process": "incident:mobile-login:resolution",
                "episode": "incident:mobile-login:episode:backend"
            },
            "current": {
                "kind": "decision",
                "summary": "Use token refresh retry instead of widening timeout.",
                "evidence": "Logs show 401 immediately after token refresh."
            },
            "semantic_delta": {
                "from": "The team suspected network timeout.",
                "to": "The evidence points to token refresh race.",
                "why": "The failing requests return 401 immediately after refresh.",
                "evidence": "Auth logs show refresh success followed by 401 on the next request."
            },
            "connect_to": [
                {
                    "ref": "incident:mobile-login:observation:401-refresh-race",
                    "rel": "chosen_because",
                    "class": "causal",
                    "why": "The decision addresses the observed token refresh race.",
                    "evidence": "The chosen retry targets the refresh race seen in auth logs.",
                    "confidence": "high"
                }
            ],
            "read_context": {
                "inspected_refs": [
                    "incident:mobile-login:observation:401-refresh-race"
                ]
            },
            "idempotency_key": "write:incident-mobile-login-decision-v1",
            "options": {
                "dry_run": true,
                "strict": true,
                "sequence": 1
            }
        })
    }

    fn valid_ingest_arguments() -> serde_json::Value {
        json!({
            "about": "incident:mobile-login",
            "idempotency_key": "ingest:incident-mobile-login:1",
            "dry_run": true,
            "memory": {
                "dimensions": [
                    {
                        "id": "incident:mobile-login",
                        "kind": "task",
                        "title": "Mobile login incident"
                    }
                ],
                "entries": [
                    {
                        "id": "incident:mobile-login:entry:decision:retry-refresh",
                        "kind": "decision",
                        "text": "Use token refresh retry instead of widening timeout.",
                        "coordinates": [
                            {
                                "dimension": "task",
                                "scope_id": "incident:mobile-login",
                                "sequence": 1,
                                "observed_at": "2026-05-06T10:00:00Z"
                            }
                        ]
                    }
                ],
                "relations": [
                    {
                        "from": "incident:mobile-login:entry:decision:retry-refresh",
                        "to": "incident:mobile-login:observation:401-refresh-race",
                        "rel": "chosen_because",
                        "class": "causal",
                        "why": "The decision addresses the observed token refresh race.",
                        "evidence": "The chosen retry targets the refresh race seen in auth logs.",
                        "confidence": "high",
                        "sequence": 1
                    }
                ],
                "evidence": [
                    {
                        "id": "evidence:incident-mobile-login:retry-refresh",
                        "supports": [
                            "incident:mobile-login:entry:decision:retry-refresh"
                        ],
                        "text": "Logs show 401 immediately after token refresh.",
                        "source": "test",
                        "time": "2026-05-06T10:00:00Z"
                    }
                ]
            },
            "provenance": {
                "source_kind": "agent",
                "source_agent": "agent:backend",
                "observed_at": "2026-05-06T10:00:00Z",
                "correlation_id": "kmp_write:incident:mobile-login",
                "causation_id": "ingest:incident-mobile-login:1"
            }
        })
    }
}
