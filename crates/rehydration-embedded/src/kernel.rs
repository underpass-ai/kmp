use std::path::{Path, PathBuf};
use std::sync::Arc;

use rehydration_adapter_embedded::EmbeddedKernelStore;
use rehydration_application::{
    CommandApplicationService, KernelMemoryApplicationService, QueryApplicationService,
    RoutingProjectionWriter, UpdateContextUseCase,
};
use rehydration_domain::PortError;

const GENERATOR_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The KMP memory facade composed over the embedded store: every port is the
/// same single-file redb store, and ingest projects synchronously in-process,
/// which is what makes `read_after_write_ready` unconditionally true.
pub type EmbeddedMemoryService = KernelMemoryApplicationService<
    EmbeddedKernelStore,
    EmbeddedKernelStore,
    EmbeddedKernelStore,
    EmbeddedKernelStore,
    RoutingProjectionWriter<Arc<EmbeddedKernelStore>, Arc<EmbeddedKernelStore>>,
>;

/// One opened embedded kernel: the composed service plus the store handle
/// for operational tooling (replay, stats, compaction).
pub struct EmbeddedKernel {
    data_dir: PathBuf,
    store: EmbeddedKernelStore,
    service: Arc<EmbeddedMemoryService>,
}

impl EmbeddedKernel {
    /// Opens the store at `data_dir` (fail-fast per ADR-012) and composes the
    /// kernel. A second session on the same data dir fails here with an
    /// explicit single-writer error (ADR-011; the engine holds the lock).
    pub fn open(data_dir: &Path) -> Result<Self, PortError> {
        let store = EmbeddedKernelStore::open(data_dir).map_err(|error| match error {
            PortError::Unavailable(message) if message.contains("could not open") => {
                PortError::Unavailable(format!(
                    "{message}; if another agent session is using this data dir, close it first \
                     (the embedded store is single-writer per ADR-011)"
                ))
            }
            other => other,
        })?;

        let graph = Arc::new(store.clone());
        let detail = Arc::new(store.clone());
        let query_application = Arc::new(QueryApplicationService::new(
            Arc::clone(&graph),
            Arc::clone(&detail),
            Arc::new(store.clone()),
            GENERATOR_VERSION,
        ));
        let update_context = Arc::new(UpdateContextUseCase::new_with_projection_writer(
            Arc::new(store.clone()),
            RoutingProjectionWriter::new(graph, detail),
            GENERATOR_VERSION,
        ));
        let service = Arc::new(KernelMemoryApplicationService::new(
            query_application,
            Arc::new(CommandApplicationService::new(update_context)),
        ));

        Ok(Self {
            data_dir: data_dir.to_path_buf(),
            store,
            service,
        })
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn store(&self) -> &EmbeddedKernelStore {
        &self.store
    }

    pub fn service(&self) -> Arc<EmbeddedMemoryService> {
        Arc::clone(&self.service)
    }
}
