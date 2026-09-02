//! The MCP server over one backend: construction and composition. Its
//! dispatch lives beside it, split by concern.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::serving::environment::{GRPC_ENDPOINT_ENV, MCP_BACKEND_ENV};
use crate::serving::grpc_tls_config::KernelMcpGrpcTlsConfig;
use crate::serving::ports::kernel_tool_backend::KernelMcpToolBackend;

use crate::serving::GrpcKernelMcpBackend;
use crate::serving::adapters::fixture_backend::FixtureKernelMcpBackend;

pub struct KernelMcpServer {
    pub(super) backend: Arc<dyn KernelMcpToolBackend>,
    /// Shared store-use claim held until this MCP transport exits. Selective
    /// uninstall must acquire the exclusive counterpart before it can remove
    /// the directory.
    store_session_lease: Option<crate::lifecycle::StoreSessionLease>,
    /// The storage engine under an embedded backend, for the startup line
    /// and the doctor. `None` for the other backends.
    pub(super) embedded_engine: Option<kmp_embedded::StorageEngine>,
    /// Where this process's viewer is serving, when it mounted one.
    pub(super) viewer_url: Option<String>,
    /// Whether the viewer has already been offered on this session. The
    /// invitation is worth saying once and is noise said twice, and the
    /// moment worth saying it at is the first memory the session writes:
    /// before that there is nothing to look at.
    pub(super) viewer_offered: AtomicBool,
    /// Selection found a repository bundle beside an unopenable project
    /// store, while this session writes to the shared user store instead.
    pub(super) orphaned_bundle: Option<kmp_embedded::OrphanedProjectBundle>,
    /// The durability loss is actionable once and noisy thereafter.
    pub(super) orphaned_bundle_offered: AtomicBool,
    pub(super) apps_negotiated: AtomicBool,
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
        let backend = crate::serving::EmbeddedKernelMcpBackend::open_with_engine(data_dir, engine)?;
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
    pub fn with_embedded_backend(backend: crate::serving::EmbeddedKernelMcpBackend) -> Self {
        let engine = backend.engine();
        let mut server = Self::with_backend(backend);
        server.embedded_engine = Some(engine);
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
        self.viewer_url = Some(url.into());
        self
    }

    pub fn with_store_session_lease(mut self, lease: crate::lifecycle::StoreSessionLease) -> Self {
        self.store_session_lease = Some(lease);
        self
    }

    /// The viewer link, returned once per session and never again. Takes the
    /// flag before returning it, so two concurrent writes cannot both claim
    /// to be the first.
    pub(super) fn viewer_invitation(&self) -> Option<&str> {
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
                        crate::lifecycle::StoreSessionLease::acquire(&data_home, resolved.path())
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
                    let catalog = crate::lifecycle::FilesystemStoreCatalog::new(&data_home);
                    let index = crate::lifecycle::JsonlStoreIndex::new(&data_home);
                    crate::lifecycle::RememberStore::new(&catalog, &index).execute(resolved.path());
                }
                let commit_native = kmp_embedded::CommitNativeBundle::for_resolved(&resolved);
                let server = Self::with_retrying_embedded_backend(
                    crate::serving::RetryingEmbeddedKernelMcpBackend::new_with_commit_native(
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

    /// Whether this server's `kmp_ask` bridges languages inside the kernel.
    pub fn bridges_languages(&self) -> bool {
        self.backend.bridges_languages()
    }
}
