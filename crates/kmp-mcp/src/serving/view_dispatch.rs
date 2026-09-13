//! The view tools' dispatch: they never reach the backend's write path —
//! a view is a camera position, not a record — but every ref an intent
//! names is checked against the store through the same read an agent uses.
//!
//! What the two tools do with that answer differs on purpose. An open onto
//! an about this store does not hold fails; an intent naming a ref it does
//! not hold degrades around it. See [`crate::serving::view_tools::open`] for
//! why ([#443](https://github.com/underpass-ai/kmp/issues/443)).

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use kmp_viewer::ViewRegistry;
use serde_json::Value;

use crate::serving::json_rpc::jsonrpc_result;
use crate::serving::kernel_mcp_server::KernelMcpServer;
use crate::serving::telemetry::{ToolErrorKind, record_tool_error, record_tool_success};
use crate::serving::tool_error::ToolError;
use crate::serving::tool_result::{tool_error_result, tool_success_result};

impl KernelMcpServer {
    /// Opens, reads or moves a view, after checking every about and ref the
    /// call names against this store. An open refuses what it cannot find;
    /// an intent degrades around it and reports it.
    pub(super) async fn handle_view_tool(
        &self,
        id: Value,
        name: &str,
        arguments: &Value,
        start: Instant,
    ) -> String {
        let outcome = match name {
            "kmp_view_get_state" => {
                crate::serving::view_tools::get_state(arguments, self.viewer_url.as_deref())
            }
            "kmp_view_undo" => crate::serving::view_tools::undo(arguments),
            "kmp_view_take_control" => crate::serving::view_tools::take_control(arguments),
            "kmp_view_open" => {
                let about = arguments.get("about").and_then(Value::as_str).unwrap_or("");
                match self.memory_ref_exists(about, about).await {
                    Ok(exists) => crate::serving::view_tools::open(
                        arguments,
                        exists,
                        self.viewer_url.as_deref(),
                    ),
                    Err(error) => Err(error),
                }
            }
            "kmp_view_apply_intent" => match self.unhonored_refs(arguments).await {
                Err(error) => Err(error),
                Ok(missing) => {
                    let abouts =
                        crate::serving::view_tools::honored_abouts(arguments, missing.as_slice());
                    match self.unhonored_projection(arguments, &abouts).await {
                        Ok(unhonored) => {
                            crate::serving::view_tools::apply_intent(arguments, &missing, unhonored)
                        }
                        Err(error) => Err(error),
                    }
                }
            },
            other => Err(ToolError::unknown_tool(format!(
                "unknown view tool `{other}`"
            ))),
        };

        match outcome {
            Ok(result) => {
                let payload = tool_success_result(result);
                record_tool_success(
                    self.backend_name(),
                    self.grpc_tls_mode_name(),
                    name,
                    arguments,
                    &payload,
                    start.elapsed(),
                );
                jsonrpc_result(id, payload)
            }
            Err(error) => {
                record_tool_error(
                    self.backend_name(),
                    self.grpc_tls_mode_name(),
                    name,
                    arguments,
                    ToolErrorKind::Validation,
                    &error.message,
                    start.elapsed(),
                );
                jsonrpc_result(id, tool_error_result(name, arguments, &error))
            }
        }
    }

    /// Every ref the intent names that this store does not hold, asked
    /// through the same read an agent would use. Collecting the whole answer
    /// rather than stopping at the first absence is what lets
    /// [`crate::serving::view_tools::apply_intent`] degrade around them and
    /// name each one, instead of collapsing the call
    /// ([#443](https://github.com/underpass-ai/kmp/issues/443)). A ref whose
    /// owner is not among the planes the intent projects is still an error:
    /// that is a scope mistake, not an absence.
    async fn unhonored_refs(
        &self,
        arguments: &Value,
    ) -> Result<crate::serving::view_tools::UnhonoredRefs, ToolError> {
        let mut missing = Vec::new();
        // An about is checked as an anchor, exactly as kmp_view_open checks
        // it, never as a ref inside some other about's scope.
        let target = arguments
            .pointer("/target/about")
            .and_then(Value::as_str)
            .map(str::to_string);
        if let Some(about) = target.as_deref()
            && !self.memory_ref_exists(about, about).await?
        {
            missing.push(about.to_string());
        }
        if let Some(layers) = arguments
            .pointer("/projection/abouts")
            .and_then(Value::as_array)
        {
            for layer in layers {
                let Some(name) = layer.as_str() else {
                    return Err(ToolError::invalid_argument(
                        "projection.abouts holds about identifiers",
                    ));
                };
                if !self.memory_ref_exists(name, name).await? {
                    missing.push(name.to_string());
                }
            }
        }

        let abouts = crate::serving::view_tools::honored_abouts(arguments, &missing);
        for reference in crate::serving::view_tools::refs_named(arguments) {
            // Without an owner this read cannot reach, nothing is checked and
            // nothing is claimed absent; the aggregate answers an intent on a
            // view that was never opened.
            if abouts.is_empty() {
                break;
            }
            if target.as_deref() == Some(reference.as_str())
                || missing.iter().any(|absent| absent == &reference)
            {
                continue;
            }
            if !self
                .memory_ref_exists_in_abouts(&abouts, &reference)
                .await?
            {
                missing.push(reference);
            }
        }
        Ok(crate::serving::view_tools::UnhonoredRefs::new(missing))
    }

    /// Whether one ref is in this store, asked through the same read the
    /// agent would use. Never a write.
    async fn memory_ref_exists(&self, about: &str, reference: &str) -> Result<bool, ToolError> {
        let reference = reference.trim();
        if reference.is_empty() {
            return Ok(false);
        }
        match self
            .backend
            .call_tool(
                "kmp_inspect",
                &serde_json::json!({
                    "about": about,
                    "ref": reference,
                    "include": {"incoming": false, "outgoing": false, "details": false}
                }),
            )
            .await
        {
            Ok(_) => Ok(true),
            Err(error)
                if error.code == crate::serving::tool_error_code::ToolErrorCode::NotFound =>
            {
                Ok(false)
            }
            Err(error) => Err(ToolError::new(
                error.code,
                format!(
                    "could not check whether `{reference}` is in this store: {}",
                    error.message
                ),
            )),
        }
    }

    async fn memory_ref_exists_in_abouts(
        &self,
        abouts: &[String],
        reference: &str,
    ) -> Result<bool, ToolError> {
        let mut scope_error = None;
        for about in abouts {
            match self.memory_ref_exists(about, reference).await {
                Ok(true) => return Ok(true),
                Ok(false) => {}
                // Inspect rejects another owner's ref as an invalid argument.
                // Try the other explicitly projected owners, never parse the
                // opaque ref or widen this read to the whole store.
                Err(error)
                    if error.code
                        == crate::serving::tool_error_code::ToolErrorCode::InvalidArgument =>
                {
                    scope_error.get_or_insert(error);
                }
                Err(error) => return Err(error),
            }
        }
        match scope_error {
            Some(error) => Err(error),
            None => Ok(false),
        }
    }

    /// What the intent asked the projection for that its owners cannot show.
    /// `abouts` are the owners the degraded intent will really have, so an
    /// about this store does not hold never scopes this read.
    async fn unhonored_projection(
        &self,
        arguments: &Value,
        abouts: &[String],
    ) -> Result<crate::serving::view_tools::UnhonoredProjection, ToolError> {
        let requested = crate::serving::view_tools::projection_names(arguments);
        let mut unhonored = crate::serving::view_tools::UnhonoredProjection::default();

        if !requested.dimensions.is_empty() || !requested.labels.is_empty() {
            let Some(about) = abouts.first() else {
                unhonored.dimensions = requested.dimensions;
                unhonored.label_keys = requested
                    .labels
                    .iter()
                    .map(|selector| selector.key.clone())
                    .collect();
                return Ok(unhonored);
            };
            // One atlas over the explicitly projected owners answers which
            // named lanes they hold (`coverage.missing`) and which
            // labels it holds at all (`labels`), the catalogue the projection
            // reads before its own filter.
            let mut dimensions = serde_json::json!({ "scope": "abouts", "abouts": abouts });
            if !requested.dimensions.is_empty() {
                dimensions["mode"] = serde_json::json!("only");
                dimensions["include"] = serde_json::json!(requested.dimensions);
            }
            let response = self
                .backend
                .call_tool(
                    "kmp_view_read_projection",
                    &serde_json::json!({
                        "about": about,
                        "from": "0001-01-01T00:00:00Z",
                        "to": "9999-12-31T23:59:59Z",
                        "lod": "atlas",
                        "dimensions": dimensions
                    }),
                )
                .await
                .map_err(|error| {
                    ToolError::new(
                        error.code,
                        format!("could not resolve the view's dimensions: {}", error.message),
                    )
                })?;
            unhonored.dimensions = response
                .pointer("/structuredContent/coverage/missing")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            let catalogue = label_catalogue(&response);
            for selector in &requested.labels {
                let Some(values) = catalogue.get(&selector.key) else {
                    if !unhonored.label_keys.contains(&selector.key) {
                        unhonored.label_keys.push(selector.key.clone());
                    }
                    continue;
                };
                let missing = selector
                    .values
                    .iter()
                    .filter(|value| !values.contains(*value))
                    .cloned()
                    .collect::<Vec<_>>();
                if !missing.is_empty() {
                    unhonored
                        .label_values
                        .entry(selector.key.clone())
                        .or_default()
                        .extend(missing);
                }
            }
        }
        for overlay in requested.overlays {
            if !ViewRegistry::shared().overlay_available(&overlay) {
                unhonored.overlays.push(overlay);
            }
        }
        Ok(unhonored)
    }
}

/// The about's labels as the projection lists them: key to the bare values
/// it holds, which is how a selector names them.
fn label_catalogue(response: &Value) -> BTreeMap<String, BTreeSet<String>> {
    let mut catalogue = BTreeMap::<String, BTreeSet<String>>::new();
    for label in response
        .pointer("/structuredContent/labels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let (Some(key), Some(value)) = (
            label.get("dimension").and_then(Value::as_str),
            label.get("value").and_then(Value::as_str),
        ) else {
            continue;
        };
        catalogue
            .entry(key.to_string())
            .or_default()
            .insert(value.to_string());
    }
    catalogue
}
