use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use serde_json::Value;

use crate::backend::{
    GRPC_ENDPOINT_ENV, KernelMcpGrpcTlsConfig, KernelMcpToolBackend, KernelMcpToolFuture,
    MCP_BACKEND_ENV,
};
use crate::fixture::FixtureKernelMcpBackend;
use crate::grpc::GrpcKernelMcpBackend;
use crate::observability::{ToolErrorKind, record_tool_error, record_tool_success};
use crate::protocol::{
    initialize_result, jsonrpc_error, jsonrpc_result, reject_unknown_arguments, tool_error_result,
    tool_success_result, tools_list_result,
};
use crate::tool_error::ToolError;
use crate::write::{build_write_plan_with_root, write_commit_result, write_dry_run_result};

pub struct KernelMcpServer {
    backend: Arc<dyn KernelMcpToolBackend>,
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
            embedded_engine: None,
            viewer_url: None,
            viewer_offered: AtomicBool::new(false),
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
                tracing::info!(
                    data_dir = %resolved.path().display(),
                    rule = resolved.rule_name(),
                    requested_engine = engine.map(|engine| engine.name()),
                    "embedded backend data dir resolved"
                );
                Ok(Self::with_retrying_embedded_backend(
                    crate::embedded::RetryingEmbeddedKernelMcpBackend::new(resolved.path(), engine),
                ))
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
                jsonrpc_result(
                    id,
                    initialize_result(self.backend_name(), self.grpc_tls_mode_name()),
                )
            }),
            Some("notifications/initialized") => None,
            Some("tools/list") => id.map(|id| jsonrpc_result(id, tools_list_result())),
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

    async fn handle_tool_call(&self, id: Value, params: Option<&Value>) -> String {
        let Some(params) = params.and_then(Value::as_object) else {
            return jsonrpc_error(id, -32602, "tools/call requires object params");
        };
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return jsonrpc_error(id, -32602, "tools/call requires params.name");
        };
        let arguments = params.get("arguments").unwrap_or(&Value::Null);
        let start = Instant::now();

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

        if name == "kernel_write_memory" {
            return self.handle_kernel_write_memory(id, arguments, start).await;
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

    async fn handle_kernel_write_memory(
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
                    "kernel_write_memory",
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
                    "kernel_write_memory",
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
                "kernel_write_memory",
                arguments,
                &result,
                start.elapsed(),
            );
            return jsonrpc_result(id, result);
        }

        match self
            .backend
            .call_tool("kernel_ingest", &plan.ingest_arguments)
            .await
        {
            Ok(result) => {
                let ingest_result = result.get("structuredContent").cloned().unwrap_or(result);
                let result = tool_success_result(write_commit_result(
                    &plan,
                    ingest_result,
                    self.viewer_invitation(),
                ));
                record_tool_success(
                    self.backend_name(),
                    self.grpc_tls_mode_name(),
                    "kernel_write_memory",
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
                    "kernel_write_memory",
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
            .call_tool("kernel_inspect", &serde_json::json!({"ref": about}))
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
                    "kernel_write_memory could not verify whether `{about}` is a new about: {}",
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
