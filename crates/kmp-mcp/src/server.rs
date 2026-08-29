use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use serde_json::Value;

use kmp_viewer::ViewRegistry;

use crate::backend::{
    GRPC_ENDPOINT_ENV, KernelMcpGrpcTlsConfig, KernelMcpToolBackend, KernelMcpToolFuture,
    MCP_BACKEND_ENV,
};
use crate::fixture::FixtureKernelMcpBackend;
use crate::grpc::GrpcKernelMcpBackend;
use crate::observability::{ToolErrorKind, record_tool_error, record_tool_success};
use crate::protocol::{
    canonical_tool_name, initialize_result_with_apps, jsonrpc_error, jsonrpc_result,
    reject_unknown_arguments, resource_read_result, resources_list_result, tool_error_result,
    tool_success_result, tools_list_result_with_apps,
};
use crate::tool_error::ToolError;
use crate::write::{build_write_plan_with_root, write_commit_result, write_dry_run_result};

fn client_supports_apps(request: &Value) -> bool {
    request
        .pointer("/params/capabilities/extensions/io.modelcontextprotocol~1ui/mimeTypes")
        .and_then(Value::as_array)
        .is_some_and(|types| {
            types
                .iter()
                .any(|value| value.as_str() == Some(crate::protocol::MCP_APP_MIME))
        })
}

pub struct KernelMcpServer {
    backend: Arc<dyn KernelMcpToolBackend>,
    /// Shared store-use claim held until this MCP transport exits. Selective
    /// uninstall must acquire the exclusive counterpart before it can remove
    /// the directory.
    store_session_lease: Option<crate::uninstall::StoreSessionLease>,
    /// The storage engine under an embedded backend, for the startup line
    /// and the doctor. `None` for the other backends.
    embedded_engine: Option<kmp_embedded::StorageEngine>,
    /// Where this process's viewer is serving, when it mounted one.
    viewer_url: Option<String>,
    /// Whether the viewer has already been offered on this session. The
    /// invitation is worth saying once and is noise said twice, and the
    /// moment worth saying it at is the first memory the session writes:
    /// before that there is nothing to look at.
    viewer_offered: AtomicBool,
    /// Selection found a repository bundle beside an unopenable project
    /// store, while this session writes to the shared user store instead.
    orphaned_bundle: Option<kmp_embedded::OrphanedProjectBundle>,
    /// The durability loss is actionable once and noisy thereafter.
    orphaned_bundle_offered: AtomicBool,
    apps_negotiated: AtomicBool,
}

impl Default for KernelMcpServer {
    fn default() -> Self {
        Self::fixture()
    }
}

impl KernelMcpServer {
    pub fn fixture() -> Self {
        Self::with_backend(FixtureKernelMcpBackend)
    }

    pub fn grpc(endpoint: impl Into<String>) -> Self {
        Self::grpc_with_tls(endpoint, KernelMcpGrpcTlsConfig::disabled())
    }

    pub fn grpc_with_tls(endpoint: impl Into<String>, tls: KernelMcpGrpcTlsConfig) -> Self {
        Self::with_backend(GrpcKernelMcpBackend::new(endpoint, tls))
    }

    pub fn embedded(data_dir: &std::path::Path) -> Result<Self, String> {
        Self::embedded_with_engine(
            data_dir,
            kmp_embedded::default_engine_for_data_dir(data_dir),
        )
    }

    pub fn embedded_with_engine(
        data_dir: &std::path::Path,
        engine: Option<kmp_embedded::StorageEngine>,
    ) -> Result<Self, String> {
        let backend =
            crate::embedded::EmbeddedKernelMcpBackend::open_with_engine(data_dir, engine)?;
        let opened_engine = backend.engine();
        let mut server = Self::with_backend(backend);
        server.embedded_engine = Some(opened_engine);
        Ok(server)
    }

    pub fn with_backend(backend: impl KernelMcpToolBackend + 'static) -> Self {
        Self::with_shared_backend(Arc::new(backend))
    }

    pub fn with_shared_backend(backend: Arc<dyn KernelMcpToolBackend>) -> Self {
        Self {
            backend,
            store_session_lease: None,
            embedded_engine: None,
            viewer_url: None,
            viewer_offered: AtomicBool::new(false),
            orphaned_bundle: None,
            orphaned_bundle_offered: AtomicBool::new(false),
            apps_negotiated: AtomicBool::new(false),
        }
    }

    /// A server over an embedded backend that the caller already opened —
    /// the viewer path, which needs the kernel handle before wrapping it.
    pub fn with_embedded_backend(backend: crate::embedded::EmbeddedKernelMcpBackend) -> Self {
        let engine = backend.engine();
        let mut server = Self::with_backend(backend);
        server.embedded_engine = Some(engine);
        server
    }

    fn with_retrying_embedded_backend(
        backend: crate::embedded::RetryingEmbeddedKernelMcpBackend,
    ) -> Self {
        let engine = backend.declared_engine();
        let mut server = Self::with_backend(backend);
        server.embedded_engine = engine;
        server
    }

    /// The storage engine this server's embedded store is on, if the backend
    /// is embedded.
    pub fn embedded_engine(&self) -> Option<kmp_embedded::StorageEngine> {
        self.embedded_engine
    }

    /// Records that a viewer is serving this process's store at `url`, so a
    /// write can hand a human the link to their own graph. Without this the
    /// viewer is reachable and unmentioned, which is how it went unnoticed.
    pub fn serving_viewer_at(mut self, url: impl Into<String>) -> Self {
        self.viewer_url = Some(url.into());
        self
    }

    pub fn with_store_session_lease(mut self, lease: crate::uninstall::StoreSessionLease) -> Self {
        self.store_session_lease = Some(lease);
        self
    }

    /// The viewer link, returned once per session and never again. Takes the
    /// flag before returning it, so two concurrent writes cannot both claim
    /// to be the first.
    fn viewer_invitation(&self) -> Option<&str> {
        let url = self.viewer_url.as_deref()?;
        self.viewer_offered
            .swap(true, Ordering::SeqCst)
            .eq(&false)
            .then_some(url)
    }

    pub fn with_orphaned_bundle(
        mut self,
        orphaned_bundle: Option<kmp_embedded::OrphanedProjectBundle>,
    ) -> Self {
        self.orphaned_bundle = orphaned_bundle;
        self
    }

    fn orphaned_bundle_notice(&self) -> Option<&kmp_embedded::OrphanedProjectBundle> {
        let orphaned = self.orphaned_bundle.as_ref()?;
        self.orphaned_bundle_offered
            .swap(true, Ordering::SeqCst)
            .eq(&false)
            .then_some(orphaned)
    }

    pub fn from_env() -> Self {
        Self::try_from_env().expect("valid MCP backend configuration")
    }

    /// What to run when nobody said. The product is the embedded kernel — one
    /// binary, no service, no database, no key — so that is what an
    /// unconfigured binary serves. The one exception is an endpoint already
    /// sitting in the environment: naming a kernel to talk to *is* asking for
    /// gRPC, and it is how the cluster edition has always been selected.
    fn default_backend(endpoint: Option<&str>) -> &'static str {
        match endpoint {
            Some(endpoint) if !endpoint.trim().is_empty() => "grpc",
            _ => "embedded",
        }
    }

    pub fn try_from_env() -> Result<Self, String> {
        let endpoint = std::env::var(GRPC_ENDPOINT_ENV).ok();
        let backend = std::env::var(MCP_BACKEND_ENV)
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| Self::default_backend(endpoint.as_deref()).to_string());
        let tls = KernelMcpGrpcTlsConfig::from_env_for_endpoint(endpoint.as_deref());

        match backend.as_str() {
            "grpc" | "live" => {
                let Some(endpoint) = endpoint.filter(|endpoint| !endpoint.trim().is_empty()) else {
                    // Only reachable when someone asked for gRPC by name: with
                    // no endpoint and no variable the default is embedded.
                    return Err(format!(
                        "{MCP_BACKEND_ENV}=grpc needs {GRPC_ENDPOINT_ENV} — a kernel to talk to. \
                         Unset {MCP_BACKEND_ENV} to run the embedded kernel instead, which needs \
                         nothing."
                    ));
                };
                Ok(Self::grpc_with_tls(endpoint, tls))
            }
            "fixture" | "fixtures" => Ok(Self::fixture()),
            "embedded" => {
                let resolved =
                    kmp_embedded::resolve_data_dir_from_env().map_err(|error| error.to_string())?;
                let engine = kmp_embedded::resolve_engine_for_data_dir_from_env(resolved.path())
                    .map_err(|error| error.to_string())?;
                let lease = kmp_embedded::user_data_home()
                    .map(|data_home| {
                        crate::uninstall::StoreSessionLease::acquire(&data_home, resolved.path())
                    })
                    .transpose()?;
                tracing::info!(
                    data_dir = %resolved.path().display(),
                    rule = resolved.rule_name(),
                    requested_engine = engine.map(|engine| engine.name()),
                    "embedded backend data dir resolved"
                );
                // Remembered so `info` can list it from any other directory
                // later: a project `.kernel` can be anywhere on disk, and
                // nothing that shipped could find one you were not standing
                // next to.
                if let Some(data_home) = kmp_embedded::user_data_home() {
                    crate::memories::remember(&data_home, resolved.path());
                }
                let commit_native = kmp_embedded::CommitNativeBundle::for_resolved(&resolved);
                let server = Self::with_retrying_embedded_backend(
                    crate::embedded::RetryingEmbeddedKernelMcpBackend::new_with_commit_native(
                        resolved.path(),
                        engine,
                        commit_native,
                    ),
                )
                .with_orphaned_bundle(resolved.orphaned_bundle().cloned());
                Ok(match lease {
                    Some(lease) => server.with_store_session_lease(lease),
                    None => server,
                })
            }
            other => Err(format!(
                "unsupported {MCP_BACKEND_ENV} value `{other}`; use `embedded` (the default), \
                 `grpc` or `fixture`"
            )),
        }
    }

    pub fn from_optional_endpoint(endpoint: Option<String>) -> Self {
        Self::from_optional_endpoint_and_tls(endpoint, KernelMcpGrpcTlsConfig::disabled())
    }

    pub fn from_optional_endpoint_and_tls(
        endpoint: Option<String>,
        tls: KernelMcpGrpcTlsConfig,
    ) -> Self {
        match endpoint.filter(|endpoint| !endpoint.trim().is_empty()) {
            Some(endpoint) => Self::grpc_with_tls(endpoint, tls),
            None => Self::fixture(),
        }
    }

    pub fn backend_name(&self) -> &'static str {
        self.backend.backend_name()
    }

    pub fn grpc_tls_mode_name(&self) -> &'static str {
        self.backend.grpc_tls_mode_name()
    }

    pub async fn handle_json_line(&self, line: &str) -> Option<String> {
        let request = match serde_json::from_str::<Value>(line) {
            Ok(request) => request,
            Err(error) => {
                return Some(jsonrpc_error(
                    Value::Null,
                    -32700,
                    &format!("invalid JSON-RPC message: {error}"),
                ));
            }
        };

        let id = request.get("id").cloned();
        let method = request.get("method").and_then(Value::as_str);

        match method {
            Some("initialize") => id.map(|id| {
                let apps = client_supports_apps(&request);
                self.apps_negotiated.store(apps, Ordering::SeqCst);
                jsonrpc_result(
                    id,
                    initialize_result_with_apps(
                        self.backend_name(),
                        self.grpc_tls_mode_name(),
                        apps,
                    ),
                )
            }),
            Some("notifications/initialized") => None,
            Some("tools/list") => id.map(|id| {
                jsonrpc_result(
                    id,
                    tools_list_result_with_apps(self.apps_negotiated.load(Ordering::SeqCst)),
                )
            }),
            Some("resources/list") if self.apps_negotiated.load(Ordering::SeqCst) => {
                id.map(|id| jsonrpc_result(id, resources_list_result()))
            }
            Some("resources/read") if self.apps_negotiated.load(Ordering::SeqCst) => id.map(|id| {
                let uri = request
                    .get("params")
                    .and_then(|params| params.get("uri"))
                    .and_then(Value::as_str);
                match uri {
                    Some(uri) => match resource_read_result(uri) {
                        Ok(result) => jsonrpc_result(id, result),
                        Err(error) => jsonrpc_error(id, -32002, &error.message),
                    },
                    None => jsonrpc_error(id, -32602, "resources/read requires params.uri"),
                }
            }),
            Some("tools/call") => match id {
                Some(id) => Some(self.handle_tool_call(id, request.get("params")).await),
                None => None,
            },
            Some(other) => id.map(|id| {
                jsonrpc_error(
                    id,
                    -32601,
                    &format!("unsupported JSON-RPC method `{other}`"),
                )
            }),
            None => Some(jsonrpc_error(
                Value::Null,
                -32600,
                "missing JSON-RPC method",
            )),
        }
    }

    /// Handles one newline-delimited JSON-RPC message without trusting the
    /// host to supply UTF-8. A broken line receives the standard parse error;
    /// it does not terminate the stdio session or discard later requests.
    pub async fn handle_json_bytes(&self, line: &[u8]) -> Option<String> {
        match std::str::from_utf8(line) {
            Ok(line) => self.handle_json_line(line).await,
            Err(error) => Some(jsonrpc_error(
                Value::Null,
                -32700,
                &format!("invalid JSON-RPC message: input is not valid UTF-8: {error}"),
            )),
        }
    }

    async fn handle_tool_call(&self, id: Value, params: Option<&Value>) -> String {
        let Some(params) = params.and_then(Value::as_object) else {
            return jsonrpc_error(id, -32602, "tools/call requires object params");
        };
        let Some(requested_name) = params.get("name").and_then(Value::as_str) else {
            return jsonrpc_error(id, -32602, "tools/call requires params.name");
        };
        let name = canonical_tool_name(requested_name);
        let arguments = params.get("arguments").unwrap_or(&Value::Null);
        let start = Instant::now();

        if matches!(name, "kmp_view_read_projection" | "kmp_view_undo")
            && !self.apps_negotiated.load(Ordering::SeqCst)
        {
            return jsonrpc_result(
                id,
                tool_error_result(&ToolError::unknown_tool(format!(
                    "{name} is callable only by a negotiated MCP App"
                ))),
            );
        }

        // Before anything reads them: the schemas declare
        // `additionalProperties: false`, so an argument the tool does not have
        // is refused here rather than dropped and answered anyway.
        if let Err(error) = reject_unknown_arguments(name, arguments) {
            record_tool_error(
                self.backend_name(),
                self.grpc_tls_mode_name(),
                name,
                arguments,
                ToolErrorKind::Validation,
                &error.message,
                start.elapsed(),
            );
            return jsonrpc_result(id, tool_error_result(&error));
        }

        if name == "kmp_write_memory" {
            return self.handle_kmp_write_memory(id, arguments, start).await;
        }

        // The view tools never reach the backend's write path — they hold a
        // view registry and a read-only existence check, and nothing else.
        if crate::view_tools::is_view_tool(name) {
            return self.handle_view_tool(id, name, arguments, start).await;
        }

        match self.backend.call_tool(name, arguments).await {
            Ok(result) => {
                record_tool_success(
                    self.backend_name(),
                    self.grpc_tls_mode_name(),
                    name,
                    arguments,
                    &result,
                    start.elapsed(),
                );
                jsonrpc_result(id, result)
            }
            Err(error) => {
                record_tool_error(
                    self.backend_name(),
                    self.grpc_tls_mode_name(),
                    name,
                    arguments,
                    ToolErrorKind::Backend,
                    &error.message,
                    start.elapsed(),
                );
                jsonrpc_result(id, tool_error_result(&error))
            }
        }
    }

    /// Moves a view, after checking that every ref the intent names is
    /// really in this store. A view that points at memory which is not there
    /// would draw an empty loom that looks like an answer.
    async fn handle_view_tool(
        &self,
        id: Value,
        name: &str,
        arguments: &Value,
        start: Instant,
    ) -> String {
        let outcome = match name {
            "kmp_view_get_state" => {
                crate::view_tools::get_state(arguments, self.viewer_url.as_deref())
            }
            "kmp_view_undo" => crate::view_tools::undo(arguments),
            "kmp_view_open" => {
                let about = arguments.get("about").and_then(Value::as_str).unwrap_or("");
                match self.memory_ref_exists(about, about).await {
                    Ok(exists) => {
                        crate::view_tools::open(arguments, exists, self.viewer_url.as_deref())
                    }
                    Err(error) => Err(error),
                }
            }
            "kmp_view_apply_intent" => {
                let mut missing = Vec::new();
                let mut failure = None;
                let about = crate::view_tools::about_for_intent(arguments);
                for reference in crate::view_tools::refs_named(arguments) {
                    let Some(about) = about.as_deref() else {
                        break;
                    };
                    match self.memory_ref_exists(about, &reference).await {
                        Ok(true) => {}
                        Ok(false) => missing.push(reference),
                        Err(error) => {
                            failure = Some(error);
                            break;
                        }
                    }
                }
                match failure {
                    Some(error) => Err(error),
                    None => match self.unhonored_projection(arguments).await {
                        Ok(unhonored) => {
                            crate::view_tools::apply_intent(arguments, &missing, unhonored)
                        }
                        Err(error) => Err(error),
                    },
                }
            }
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
                jsonrpc_result(id, tool_error_result(&error))
            }
        }
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
            Err(error) if error.code == crate::tool_error::ToolErrorCode::NotFound => Ok(false),
            Err(error) => Err(ToolError::new(
                error.code,
                format!(
                    "could not check whether `{reference}` is in this store: {}",
                    error.message
                ),
            )),
        }
    }

    async fn unhonored_projection(
        &self,
        arguments: &Value,
    ) -> Result<crate::view_tools::UnhonoredProjection, ToolError> {
        let requested = crate::view_tools::projection_names(arguments);
        let mut unhonored = crate::view_tools::UnhonoredProjection::default();
        let about = crate::view_tools::about_for_intent(arguments);

        if !requested.dimensions.is_empty() {
            let Some(about) = about else {
                unhonored.dimensions = requested.dimensions;
                return Ok(unhonored);
            };
            let response = self
                .backend
                .call_tool(
                    "kmp_view_read_projection",
                    &serde_json::json!({
                        "about": about,
                        "from": "0001-01-01T00:00:00Z",
                        "to": "9999-12-31T23:59:59Z",
                        "lod": "atlas",
                        "dimensions": {
                            "mode": "only",
                            "include": requested.dimensions,
                            "scope": "current_about"
                        }
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
        }
        for overlay in requested.overlays {
            if !ViewRegistry::shared().overlay_available(&overlay) {
                unhonored.overlays.push(overlay);
            }
        }
        Ok(unhonored)
    }

    async fn handle_kmp_write_memory(
        &self,
        id: Value,
        arguments: &Value,
        start: Instant,
    ) -> String {
        let allow_unlinked_root = match self.allow_unlinked_strict_root(arguments).await {
            Ok(allowed) => allowed,
            Err(error) => {
                record_tool_error(
                    self.backend_name(),
                    self.grpc_tls_mode_name(),
                    "kmp_write_memory",
                    arguments,
                    ToolErrorKind::Backend,
                    &error.message,
                    start.elapsed(),
                );
                return jsonrpc_result(id, tool_error_result(&error));
            }
        };
        let plan = match build_write_plan_with_root(arguments, allow_unlinked_root) {
            Ok(plan) => plan,
            Err(message) => {
                // Everything the write planner refuses is about the
                // arguments: a missing field, an unsupported relation, a rich
                // link with no evidence. The caller can fix all of it, and
                // only the caller can.
                let error = ToolError::invalid_argument(message);
                record_tool_error(
                    self.backend_name(),
                    self.grpc_tls_mode_name(),
                    "kmp_write_memory",
                    arguments,
                    ToolErrorKind::Validation,
                    &error.message,
                    start.elapsed(),
                );
                return jsonrpc_result(id, tool_error_result(&error));
            }
        };

        if plan.dry_run {
            let result = tool_success_result(write_dry_run_result(&plan));
            record_tool_success(
                self.backend_name(),
                self.grpc_tls_mode_name(),
                "kmp_write_memory",
                arguments,
                &result,
                start.elapsed(),
            );
            return jsonrpc_result(id, result);
        }

        match self
            .backend
            .call_tool("kmp_ingest", &plan.ingest_arguments)
            .await
        {
            Ok(result) => {
                let ingest_result = result.get("structuredContent").cloned().unwrap_or(result);
                let result = tool_success_result(write_commit_result(
                    &plan,
                    ingest_result,
                    self.viewer_invitation(),
                    self.orphaned_bundle_notice(),
                ));
                record_tool_success(
                    self.backend_name(),
                    self.grpc_tls_mode_name(),
                    "kmp_write_memory",
                    arguments,
                    &result,
                    start.elapsed(),
                );
                jsonrpc_result(id, result)
            }
            Err(error) => {
                record_tool_error(
                    self.backend_name(),
                    self.grpc_tls_mode_name(),
                    "kmp_write_memory",
                    arguments,
                    ToolErrorKind::Backend,
                    &error.message,
                    start.elapsed(),
                );
                jsonrpc_result(id, tool_error_result(&error))
            }
        }
    }

    async fn allow_unlinked_strict_root(&self, arguments: &Value) -> Result<bool, ToolError> {
        let Some(object) = arguments.as_object() else {
            return Ok(false);
        };
        let strict = object
            .get("options")
            .and_then(Value::as_object)
            .and_then(|options| options.get("strict"))
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let has_links = object
            .get("connect_to")
            .and_then(Value::as_array)
            .is_some_and(|links| !links.is_empty());
        if !strict || has_links {
            return Ok(false);
        }
        let Some(about) = object
            .get("about")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|about| !about.is_empty())
        else {
            return Ok(false);
        };

        match self
            .backend
            .call_tool(
                "kmp_inspect",
                &serde_json::json!({
                    "about": about,
                    "ref": about,
                    "include": {"incoming": false, "outgoing": false, "details": false}
                }),
            )
            .await
        {
            Ok(_) => Ok(false),
            // The kernel says which failure this was, so this no longer has
            // to guess from the words — and "not found" here is the whole
            // point: an about nobody has written yet is allowed one unlinked
            // root entry.
            Err(error) if error.code == crate::tool_error::ToolErrorCode::NotFound => Ok(true),
            Err(error) => Err(ToolError::new(
                error.code,
                format!(
                    "kmp_write_memory could not verify whether `{about}` is a new about: {}",
                    error.message
                ),
            )),
        }
    }
}

impl<T> KernelMcpToolBackend for Arc<T>
where
    T: KernelMcpToolBackend + ?Sized,
{
    fn backend_name(&self) -> &'static str {
        self.as_ref().backend_name()
    }

    fn grpc_tls_mode_name(&self) -> &'static str {
        self.as_ref().grpc_tls_mode_name()
    }

    fn call_tool<'a>(&'a self, name: &'a str, arguments: &'a Value) -> KernelMcpToolFuture<'a> {
        self.as_ref().call_tool(name, arguments)
    }
}
