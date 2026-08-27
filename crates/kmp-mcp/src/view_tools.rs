//! The agent's half of ChronoLoom: three tools that move a view, and nothing
//! else.
//!
//! The principle these enforce is the whole reason they are small: an agent
//! controls *semantic view intentions*, never DOM events, screen coordinates
//! or code. It cannot say "move the camera to x=438"; it says "focus these
//! refs, on occurred time, between these two instants, showing causal and
//! evidential relations" — and the loom decides what that looks like.
//!
//! They never write memory. `kmp_write_memory` and `kmp_ingest` are not
//! reachable from a view action, by construction: these handlers hold a view
//! registry and a read-only validator, and nothing else.

use serde_json::{Value, json};

use kmp_viewer::{
    DEFAULT_VIEW_ID, Focus, Projection, TimeRange, TraceSelection, ViewError, ViewPatch,
    ViewRegistry, ViewState,
};

use crate::tool_error::ToolError;

pub(crate) const VIEW_TOOLS: [&str; 3] = [
    "kmp_view_open",
    "kmp_view_apply_intent",
    "kmp_view_get_state",
];

pub(crate) fn is_view_tool(name: &str) -> bool {
    VIEW_TOOLS.contains(&name) || name == "kmp_view_undo"
}

fn view_id_of(arguments: &Value) -> String {
    arguments
        .get("view_id")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_VIEW_ID)
        .to_string()
}

fn state_result(state: &ViewState, extra: Value) -> Value {
    let mut result = json!({
        "view_id": state.view_id,
        "view_revision": state.view_revision,
        "state": state,
    });
    if let (Some(result), Some(extra)) = (result.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            result.insert(key.clone(), value.clone());
        }
    }
    result
}

/// Opens or rehydrates a view. The `about` must exist: a view onto memory
/// that is not there would render an empty loom that looks like an answer.
pub(crate) fn open(arguments: &Value, about_exists: bool) -> Result<Value, ToolError> {
    let Some(about) = arguments.get("about").and_then(Value::as_str) else {
        return Err(ToolError::invalid_argument(
            "kmp_view_open needs `about`: the memory the loom should weave",
        ));
    };
    if !about_exists {
        return Err(ToolError::not_found(format!(
            "`{about}` is not an anchor in this store; opening a view onto it would render an \
             empty loom that looks like an answer"
        )));
    }
    let view_id = arguments.get("view_id").and_then(Value::as_str);
    let expected = arguments.get("expected_revision").and_then(Value::as_u64);
    let state = match ViewRegistry::shared().open_checked(
        view_id,
        Some(about.to_string()),
        expected,
        "agent",
        Some("opened a different about"),
    ) {
        Ok(state) => state,
        Err(ViewError::Conflict {
            expected, actual, ..
        }) => {
            return Err(ToolError::conflict(format!(
                "the view moved before it could be opened: expected revision {expected}, it is at {actual}"
            )));
        }
        Err(ViewError::UnknownView(id)) => {
            return Err(ToolError::not_found(format!("no view under `{id}`")));
        }
        Err(ViewError::Invalid(message)) => return Err(ToolError::invalid_argument(message)),
    };
    Ok(state_result(
        &state,
        json!({
            "opened": true,
            "clocks": kmp_viewer::view_state::CLOCKS,
            "semantic_zoom_ladder": kmp_viewer::view_state::ZOOMS,
        }),
    ))
}

/// Reads the view without changing it — semantic state, never pixels.
pub(crate) fn get_state(arguments: &Value) -> Result<Value, ToolError> {
    let view_id = view_id_of(arguments);
    let Some(state) = ViewRegistry::shared().get(&view_id) else {
        return Err(ToolError::not_found(format!(
            "no view under `{view_id}` — open one with kmp_view_open first"
        )));
    };
    Ok(state_result(
        &state,
        json!({
            "reads": "the semantic state of the view, not its pixels",
            "observability": "projection.overlays are queried for the current range and drawn on the shared temporal axis",
        }),
    ))
}

/// MCP App transport for the aggregate's existing reversible operation. It
/// is never advertised to the model-facing tool surface.
pub(crate) fn undo(arguments: &Value) -> Result<Value, ToolError> {
    let view_id = view_id_of(arguments);
    match ViewRegistry::shared().undo(&view_id, "human") {
        Ok(state) => Ok(state_result(&state, json!({"undone": true}))),
        Err(ViewError::UnknownView(id)) => Err(ToolError::not_found(format!(
            "no view under `{id}` — open one with kmp_view_open first"
        ))),
        Err(ViewError::Invalid(message)) => Err(ToolError::invalid_argument(message)),
        Err(ViewError::Conflict { .. }) => Err(ToolError::conflict(
            "the view moved before undo could be applied".to_string(),
        )),
    }
}

/// Builds the patch an intent describes, refusing shapes it does not mean.
fn patch_from(arguments: &Value) -> Result<(ViewPatch, Vec<String>), ToolError> {
    let mut patch = ViewPatch::default();
    let mut refs = Vec::new();

    if let Some(target) = arguments.get("target")
        && let Some(about) = target.get("about").and_then(Value::as_str)
    {
        // Checked like every other reference an intent names: kmp_view_open
        // refuses an absent about, and this is the same door.
        refs.push(about.to_string());
        patch.about = Some(about.to_string());
    }
    if let Some(focus) = arguments.get("focus") {
        let mut built = Focus::default();
        if let Some(range) = focus.get("time_range") {
            if let Some(axis) = range.get("axis").and_then(Value::as_str) {
                patch.clock = Some(axis.to_string());
            }
            built.time_range = Some(TimeRange {
                from: range
                    .get("from")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                to: range.get("to").and_then(Value::as_str).map(str::to_string),
            });
        }
        if let Some(list) = focus.get("refs").and_then(Value::as_array) {
            for entry in list {
                let Some(reference) = entry.as_str() else {
                    return Err(ToolError::invalid_argument(
                        "focus.refs holds memory refs, which are strings",
                    ));
                };
                refs.push(reference.to_string());
            }
            built.refs = refs.clone();
        }
        patch.focus = Some(built);
    }
    if let Some(projection) = arguments.get("projection") {
        let strings = |key: &str| {
            projection.get(key).and_then(Value::as_array).map(|list| {
                list.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
        };
        patch.projection = Some(Projection {
            semantic_zoom: projection
                .get("semantic_zoom")
                .and_then(Value::as_str)
                .map(str::to_string),
            dimensions: strings("dimensions"),
            relation_classes: strings("relation_classes"),
            overlays: strings("overlays"),
        });
    }
    if let Some(selection) = arguments.get("selection") {
        patch.selection = Some(match selection {
            Value::Null => None,
            Value::String(reference) => {
                refs.push(reference.clone());
                Some(reference.clone())
            }
            _ => {
                return Err(ToolError::invalid_argument(
                    "selection is a memory ref or null",
                ));
            }
        });
    }
    if let Some(trace) = arguments.get("trace") {
        patch.trace = Some(match trace {
            Value::Null => None,
            _ => {
                let (Some(from), Some(to)) = (
                    trace.get("from").and_then(Value::as_str),
                    trace.get("to").and_then(Value::as_str),
                ) else {
                    return Err(ToolError::invalid_argument(
                        "a trace needs two ends: `from` and `to`",
                    ));
                };
                refs.push(from.to_string());
                refs.push(to.to_string());
                Some(TraceSelection {
                    from: from.to_string(),
                    to: to.to_string(),
                })
            }
        });
    }
    if let Some(search) = arguments.get("search") {
        patch.search = Some(match search {
            Value::Null => None,
            Value::String(text) => Some(text.clone()),
            _ => return Err(ToolError::invalid_argument("search is text or null")),
        });
    }
    Ok((patch, refs))
}

#[derive(Debug, Default)]
pub(crate) struct ProjectionNames {
    pub(crate) dimensions: Vec<String>,
    pub(crate) overlays: Vec<String>,
}

#[derive(Debug, Default)]
pub(crate) struct UnhonoredProjection {
    pub(crate) dimensions: Vec<String>,
    pub(crate) overlays: Vec<String>,
}

pub(crate) fn projection_names(arguments: &Value) -> ProjectionNames {
    let values = |key: &str| {
        arguments
            .pointer(&format!("/projection/{key}"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    };
    ProjectionNames {
        dimensions: values("dimensions"),
        overlays: values("overlays"),
    }
}

pub(crate) fn about_for_intent(arguments: &Value) -> Option<String> {
    arguments
        .pointer("/target/about")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            ViewRegistry::shared()
                .get(&view_id_of(arguments))
                .and_then(|state| state.about)
        })
}

fn omit_unhonored_projection(patch: &mut ViewPatch, unavailable: &UnhonoredProjection) {
    let Some(projection) = patch.projection.as_mut() else {
        return;
    };
    if let Some(dimensions) = projection.dimensions.as_mut() {
        dimensions.retain(|name| !unavailable.dimensions.contains(name));
        // `Some([])` is a keep-list with no lanes. If every requested name
        // was a typo, fall back to the unfiltered projection instead of
        // turning a partial intent into an empty loom.
        if dimensions.is_empty() && !unavailable.dimensions.is_empty() {
            projection.dimensions = None;
        }
    }
    if let Some(overlays) = projection.overlays.as_mut() {
        overlays.retain(|name| !unavailable.overlays.contains(name));
    }
}

fn has_explicit_time_range(arguments: &Value) -> bool {
    arguments.pointer("/focus/time_range").is_some_and(|range| {
        range.get("from").and_then(Value::as_str).is_some()
            && range.get("to").and_then(Value::as_str).is_some()
    })
}

/// The refs an intent names, so the caller can check they exist before the
/// view points at them.
pub(crate) fn refs_named(arguments: &Value) -> Vec<String> {
    patch_from(arguments)
        .map(|(_, refs)| refs)
        .unwrap_or_default()
}

/// Applies one intent atomically: focus, clock, filters, selection, trace —
/// under optimistic concurrency and idempotency.
pub(crate) fn apply_intent(
    arguments: &Value,
    missing_refs: &[String],
    unavailable: UnhonoredProjection,
) -> Result<Value, ToolError> {
    let view_id = view_id_of(arguments);
    // A replay is answered before the revision is checked: the intent under
    // that key already landed, so this is success, not a conflict. The state
    // that comes back is the present one, which is how a caller still sees
    // that the person has since moved.
    let Some(idempotency_key) = arguments.get("idempotency_key").and_then(Value::as_str) else {
        return Err(ToolError::invalid_argument(
            "kmp_view_apply_intent needs `idempotency_key`: a retried intent must be the same \
             intent, not a second one",
        ));
    };
    if !missing_refs.is_empty() {
        return Err(ToolError::not_found(format!(
            "these refs are not in this store: {}. The loom points at memory that exists; it \
             does not draw placeholders that look like data.",
            missing_refs.join(", ")
        )));
    }
    let (mut patch, _) = patch_from(arguments)?;
    omit_unhonored_projection(&mut patch, &unavailable);
    let mut unhonored: Vec<String> = unavailable
        .dimensions
        .into_iter()
        .chain(unavailable.overlays)
        .collect();
    if arguments.get("trace").is_some_and(|trace| !trace.is_null())
        && has_explicit_time_range(arguments)
    {
        unhonored.push("trace framing (explicit focus.time_range has priority)".to_string());
    }
    let expected = arguments.get("expected_revision").and_then(Value::as_u64);
    let explanation = arguments.get("explanation").and_then(Value::as_str);
    let actor = arguments
        .get("actor")
        .and_then(Value::as_str)
        .unwrap_or("agent");

    match ViewRegistry::shared().apply_with_unhonored(
        &view_id,
        expected,
        Some(idempotency_key),
        patch,
        actor,
        explanation,
        unhonored,
    ) {
        Ok(applied) => Ok(state_result(
            &applied.state,
            json!({
                "applied": applied.applied,
                "unhonored": applied.unhonored,
            }),
        )),
        Err(ViewError::Conflict {
            expected,
            actual,
            current,
        }) => Err(ToolError::conflict(format!(
            "the view moved while this intent was being prepared: it expected revision \
             {expected} and the view is at {actual}. Rebase on the state in \
             kmp_view_get_state and apply again — the person at the loom has right of way. \
             Their clock is `{}`, their selection is {}.",
            current.clock,
            current
                .selection
                .as_deref()
                .map(|reference| format!("`{reference}`"))
                .unwrap_or_else(|| "nothing".to_string())
        ))),
        Err(ViewError::UnknownView(id)) => Err(ToolError::not_found(format!(
            "no view under `{id}` — open one with kmp_view_open first"
        ))),
        Err(ViewError::Invalid(message)) => Err(ToolError::invalid_argument(message)),
    }
}
