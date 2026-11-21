//! The MCP server over one backend: construction and composition. Its
//! dispatch lives beside it, split by concern.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::serving::environment::{GRPC_ENDPOINT_ENV, MCP_BACKEND_ENV};
use crate::serving::grpc_tls_config::KernelMcpGrpcTlsConfig;
use crate::serving::ports::kernel_tool_backend::KernelMcpToolBackend;

use crate::serving::GrpcKernelMcpBackend;
#[cfg(any(debug_assertions, feature = "eval-hooks"))]
use crate::serving::adapters::fixture_backend::FixtureKernelMcpBackend;

/// Retained read continuations beside the store. Version 0.27.0 kept them in
/// `agent-users.sqlite3` with agent identities; that file is no longer read.
const CONTINUATIONS_FILE: &str = "continuations.sqlite3";

pub struct KernelMcpServer {
    pub(super) shared_passages: bool,
    /// Advertise each tool's `outputSchema` in `tools/list` (off by default).
    pub(super) output_schemas: bool,
    pub(super) backend: Arc<dyn KernelMcpToolBackend>,
    /// Retained read continuations, opened on first use.
    pub(super) continuation_store:
        std::sync::OnceLock<Arc<dyn crate::continuation::ContinuationStore>>,
    /// Where they are kept; `None` keeps them in memory for this process.
    pub(super) continuation_store_path: Option<std::path::PathBuf>,
    /// Shared store-use claim held until this MCP transport exits. Selective
    /// uninstall must acquire the exclusive counterpart before it can remove
    /// the directory.
    store_session_lease: Option<crate::lifecycle::StoreSessionLease>,
    /// The storage engine under an embedded backend, for the startup line
    /// and the doctor. `None` for the other backends.
    pub(super) embedded_engine: Option<kmp_embedded::StorageEngine>,
    /// This session's viewer, when it may have one. It listens only from
    /// the first request that asks for it.
    pub(super) viewer: Option<std::sync::Arc<crate::viewer::SessionViewer>>,
    /// Whether the viewer has already been offered on this session. The
    /// invitation is worth saying once and is noise said twice, and the
    /// moment worth saying it at is the first memory the session writes:
    /// before that there is nothing to look at.
    pub(super) viewer_offered: AtomicBool,
    /// Selection found a repository bundle beside an unopenable project
    /// store, while this session writes to the shared user store instead.
    pub(super) orphaned_bundle: Option<kmp_embedded::OrphanedProjectBundle>,
    /// Which store an embedded session opened and why, for the agent.
    pub(super) store_disclosure: Option<crate::serving::store_disclosure::StoreDisclosure>,
    /// The durability loss is actionable once and noisy thereafter.
    pub(super) orphaned_bundle_offered: AtomicBool,
    /// This process's own session (stdio). The HTTP gateway passes its own
    /// per request instead.
    pub(super) session: crate::serving::mcp_session::McpSession,
    /// Where this store keeps the salt its call fingerprints are keyed
    /// with; `None` (fixtures) fingerprints nothing.
    pub(super) telemetry_salt_path: Option<std::path::PathBuf>,
    /// The salt, read or created on the first wake or ask that succeeds;
    /// `None` inside once it could not be, so no call retries it.
    pub(super) telemetry_salt:
        std::sync::OnceLock<Option<Arc<crate::serving::telemetry::FingerprintSalt>>>,
}

/// The canned-answer server the test suite drives. Debug and `eval-hooks`
/// builds only: a release build has no fixture backend.
#[cfg(any(debug_assertions, feature = "eval-hooks"))]
impl Default for KernelMcpServer {
    fn default() -> Self {
        Self::fixture()
    }
}

impl KernelMcpServer {
    /// Canned answers that look real and store nothing. Debug and
    /// `eval-hooks` builds only.
    #[cfg(any(debug_assertions, feature = "eval-hooks"))]
    pub fn fixture() -> Self {
        Self::with_backend(FixtureKernelMcpBackend)
    }

    pub fn grpc(endpoint: impl Into<String>) -> Self {
        Self::grpc_with_tls(endpoint, KernelMcpGrpcTlsConfig::disabled())
    }

    pub fn grpc_with_tls(endpoint: impl Into<String>, tls: KernelMcpGrpcTlsConfig) -> Self {
        use sha2::{Digest, Sha256};
        let endpoint = endpoint.into();
        let key = format!("{:x}", Sha256::digest(endpoint.as_bytes()));
        let mut server = Self::with_backend(GrpcKernelMcpBackend::new(endpoint, tls));
        // A remote store's continuations and salt stay on this machine.
        server.continuation_store_path = kmp_embedded::user_data_home().map(|home| {
            home.join("agent-users")
                .join(format!("{key}.{CONTINUATIONS_FILE}"))
        });
        server.telemetry_salt_path = kmp_embedded::user_data_home().map(|home| {
            home.join("agent-users").join(format!(
                "{key}.{}",
                crate::serving::telemetry::TELEMETRY_SALT_FILE
            ))
        });
        server
    }

    /// Keeps a remote store's retained continuations and salt under
    /// `data_home` instead of the user data home, the way a hermetic caller
    /// (a test) needs (#910).
    pub fn with_remote_state_home(mut self, data_home: &std::path::Path) -> Self {
        let relocate = |path: Option<std::path::PathBuf>| {
            path.and_then(|path| {
                path.file_name()
                    .map(|name| data_home.join("agent-users").join(name))
            })
        };
        self.continuation_store_path = relocate(self.continuation_store_path.take());
        self.telemetry_salt_path = relocate(self.telemetry_salt_path.take());
        self
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
        let backend = crate::serving::EmbeddedKernelMcpBackend::open_with_engine(data_dir, engine)?;
        let opened_engine = backend.engine();
        let mut server = Self::with_backend(backend);
        server.embedded_engine = Some(opened_engine);
        server.continuation_store_path = Some(data_dir.join(CONTINUATIONS_FILE));
        server.telemetry_salt_path =
            Some(data_dir.join(crate::serving::telemetry::TELEMETRY_SALT_FILE));
        Ok(server)
    }

    pub fn with_backend(backend: impl KernelMcpToolBackend + 'static) -> Self {
        Self::with_shared_backend(Arc::new(backend))
    }

    pub fn with_shared_backend(backend: Arc<dyn KernelMcpToolBackend>) -> Self {
        Self {
            shared_passages: false,
            output_schemas: false,
            backend,
            continuation_store: std::sync::OnceLock::new(),
            continuation_store_path: None,
            store_session_lease: None,
            embedded_engine: None,
            viewer: None,
            viewer_offered: AtomicBool::new(false),
            orphaned_bundle: None,
            store_disclosure: None,
            orphaned_bundle_offered: AtomicBool::new(false),
            session: crate::serving::mcp_session::McpSession::new(),
            telemetry_salt_path: None,
            telemetry_salt: std::sync::OnceLock::new(),
        }
    }

    /// A server over an embedded backend that the caller already opened —
    /// the viewer path, which needs the kernel handle before wrapping it.
    pub fn with_embedded_backend(backend: crate::serving::EmbeddedKernelMcpBackend) -> Self {
        let engine = backend.engine();
        let data_dir = std::path::Path::new(backend.data_dir());
        let continuations = data_dir.join(CONTINUATIONS_FILE);
        let salt = data_dir.join(crate::serving::telemetry::TELEMETRY_SALT_FILE);
        let mut server = Self::with_backend(backend);
        server.embedded_engine = Some(engine);
        server.continuation_store_path = Some(continuations);
        server.telemetry_salt_path = Some(salt);
        server
    }

    fn with_retrying_embedded_backend(
        backend: crate::serving::RetryingEmbeddedKernelMcpBackend,
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
        self.viewer = Some(std::sync::Arc::new(
            crate::viewer::SessionViewer::already_serving(url),
        ));
        self
    }

    /// Mounts a viewer that binds its port on the first request for it, so
    /// a session nobody looks at listens on nothing.
    pub fn with_viewer_on_demand(mut self, viewer: crate::viewer::SessionViewer) -> Self {
        self.viewer = Some(std::sync::Arc::new(viewer));
        self
    }

    /// The viewer link when the viewer is already listening; never starts it.
    pub(super) fn viewer_url(&self) -> Option<&str> {
        self.viewer.as_deref()?.url()
    }

    pub fn with_store_session_lease(mut self, lease: crate::lifecycle::StoreSessionLease) -> Self {
        self.store_session_lease = Some(lease);
        self
    }

    /// The viewer link, returned once per session and never again. Takes the
    /// flag before returning it, so two concurrent writes cannot both claim
    /// to be the first.
    pub(super) fn viewer_invitation(&self) -> Option<&str> {
        let url = self.viewer_url()?;
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

    /// Everything the session tells its agent about the store selection:
    /// which store opened, by which rule, and a bundle it no longer maintains.
    pub fn with_resolved_store(self, resolved: &kmp_embedded::ResolvedDataDir) -> Self {
        let mut server = self.with_orphaned_bundle(resolved.orphaned_bundle().cloned());
        server.store_disclosure = Some(crate::serving::store_disclosure::StoreDisclosure::of(
            resolved,
        ));
        server
    }

    pub(crate) fn store_disclosure(
        &self,
    ) -> Option<&crate::serving::store_disclosure::StoreDisclosure> {
        self.store_disclosure.as_ref()
    }

    /// A wake carries the store it read, beside its result or its refusal:
    /// "not found" is exactly the answer that means nothing without it.
    pub(super) fn disclose_store(
        &self,
        tool: &str,
        mut result: serde_json::Value,
    ) -> serde_json::Value {
        if tool != "kmp_wake" {
            return result;
        }
        let Some(store) = &self.store_disclosure else {
            return result;
        };
        if let Some(content) = result
            .get_mut("structuredContent")
            .and_then(serde_json::Value::as_object_mut)
        {
            content.insert("store".to_string(), store.field());
        }
        if result.get("isError") == Some(&serde_json::Value::Bool(true))
            && let Some(text) = result
                .pointer_mut("/content/0/text")
                .and_then(|text| text.as_str().map(str::to_string))
        {
            result["content"][0]["text"] =
                serde_json::json!(format!("{}{text}", store.error_prefix()));
        }
        result
    }

    /// A wake that found nothing names the store in its text too: "nothing
    /// here" must say where "here" is to a host that shows only text.
    pub(super) fn disclose_store_in_text(
        &self,
        mut result: serde_json::Value,
    ) -> serde_json::Value {
        if let Some(store) = &self.store_disclosure
            && let Some(text) = result
                .pointer("/content/0/text")
                .and_then(|text| text.as_str().map(str::to_string))
        {
            result["content"][0]["text"] =
                serde_json::json!(format!("{}{text}", store.error_prefix()));
        }
        result
    }

    pub(super) fn orphaned_bundle_notice(&self) -> Option<&kmp_embedded::OrphanedProjectBundle> {
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
    fn default_backend(endpoint: Option<&str>) -> crate::serving::backend_name::BackendName {
        use crate::serving::backend_name::BackendName;
        match endpoint {
            Some(endpoint) if !endpoint.trim().is_empty() => BackendName::Grpc,
            _ => BackendName::Embedded,
        }
    }

    pub fn try_from_env() -> Result<Self, String> {
        Self::try_from_env_with_store(None)
    }

    /// [`try_from_env`](Self::try_from_env), with the embedded store chosen
    /// by the caller instead of resolved here. `kmp-mcp demo` uses it to keep
    /// its worked example out of a project's store.
    pub fn try_from_env_with_store(
        store: Option<kmp_embedded::ResolvedDataDir>,
    ) -> Result<Self, String> {
        use crate::serving::backend_name::BackendName;
        // Every switch is checked before any is read: a value the engine
        // does not accept is refused here, naming what it accepts, instead
        // of being read as the default (E18).
        crate::environment::check_environment().map_err(|refusal| refusal.to_string())?;
        let endpoint = crate::serving::environment::optional_env_string(GRPC_ENDPOINT_ENV);
        let backend = match crate::serving::environment::optional_env_string(MCP_BACKEND_ENV) {
            Some(value) => BackendName::named(&value).ok_or_else(|| {
                format!(
                    "unsupported {MCP_BACKEND_ENV} value `{value}`; it accepts {}",
                    crate::environment::BACKEND_ACCEPTED
                )
            })?,
            None => Self::default_backend(endpoint.as_deref()),
        };
        let tls = KernelMcpGrpcTlsConfig::from_env_for_endpoint(endpoint.as_deref());

        match backend {
            BackendName::Grpc => {
                let Some(endpoint) = endpoint else {
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
            #[cfg(any(debug_assertions, feature = "eval-hooks"))]
            BackendName::Fixture => Ok(Self::fixture()),
            BackendName::Embedded => {
                let resolved = match store {
                    Some(store) => crate::lifecycle::prepare_memory_for_use(store),
                    None => crate::lifecycle::resolve_memory_for_use(),
                }
                .map_err(|error| error.to_string())?;
                let engine = kmp_embedded::default_engine_for_data_dir(resolved.path());
                let lease = kmp_embedded::user_data_home()
                    .map(|data_home| {
                        crate::lifecycle::StoreSessionLease::acquire(&data_home, resolved.path())
                    })
                    .transpose()?;
                tracing::info!(
                    data_dir = %resolved.path().display(),
                    rule = resolved.rule_name(),
                    requested_engine = engine.map(|engine| engine.name()),
                    "embedded backend data dir resolved"
                );
                let commit_native = kmp_embedded::CommitNativeBundle::for_resolved_excluding_abouts(
                    &resolved,
                    crate::local_abouts(),
                );
                let mut server = Self::with_retrying_embedded_backend(
                    crate::serving::RetryingEmbeddedKernelMcpBackend::new_with_commit_native(
                        resolved.path(),
                        engine,
                        commit_native,
                    ),
                )
                .with_resolved_store(&resolved);
                // Opened lazily, on the first continuation a read returns. A
                // metadata file must not make a not-yet-created kernel look
                // non-empty.
                server.continuation_store_path = Some(resolved.path().join(CONTINUATIONS_FILE));
                server.telemetry_salt_path = Some(
                    resolved
                        .path()
                        .join(crate::serving::telemetry::TELEMETRY_SALT_FILE),
                );
                Ok(match lease {
                    Some(lease) => server.with_store_session_lease(lease),
                    None => server,
                })
            }
        }
        .and_then(Self::with_passage_format_from_env)
        .and_then(Self::with_output_schemas_from_env)
    }

    /// A gRPC server for an endpoint, the fixture without one. Debug and
    /// `eval-hooks` builds only.
    #[cfg(any(debug_assertions, feature = "eval-hooks"))]
    pub fn from_optional_endpoint(endpoint: Option<String>) -> Self {
        Self::from_optional_endpoint_and_tls(endpoint, KernelMcpGrpcTlsConfig::disabled())
    }

    #[cfg(any(debug_assertions, feature = "eval-hooks"))]
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

    /// Whether this server's `kmp_ask` bridges languages inside the kernel.
    pub fn bridges_languages(&self) -> bool {
        self.backend.bridges_languages()
    }

    /// The tools whose `openWorldHint` is true for this session.
    pub(crate) fn open_world_tools(&self) -> Vec<&'static str> {
        self.backend.open_world_tools()
    }
}
