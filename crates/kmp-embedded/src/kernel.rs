use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use kmp_adapter_embedded::{
    EmbeddedKernelStore, QualityTelemetryRetention, SqliteQualityTelemetryReader,
    SqliteQualityTelemetryWriter, StorageEngine,
};
use kmp_application::{
    CommandApplicationService, KernelMemoryApplicationService, QueryApplicationService,
    RoutingProjectionWriter, UpdateContextUseCase,
};
use kmp_domain::{PortError, QualityMetricsObserver};
use kmp_observability::{BufferedQualityMetricsObserver, EmbeddedTelemetryGuard};

const GENERATOR_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The KMP memory facade composed over one stamped embedded store: every port
/// shares the selected engine, and ingest projects synchronously in-process,
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
    engine: StorageEngine,
    store: EmbeddedKernelStore,
    service: Arc<EmbeddedMemoryService>,
    quality_observer: Arc<BufferedQualityMetricsObserver>,
    telemetry_guard: Option<EmbeddedTelemetryGuard>,
    telemetry_writer: Option<Arc<SqliteQualityTelemetryWriter>>,
    quality_telemetry_error: Option<String>,
}

impl EmbeddedKernel {
    /// Opens the store at `data_dir` (fail-fast per ADR-012) and composes the
    /// kernel. An existing directory opens with the engine it was created
    /// with; a fresh one gets the caller's resolved default. SQLite permits
    /// several sessions to share the same data directory.
    pub fn open(data_dir: &Path) -> Result<Self, PortError> {
        Self::open_with_engine(data_dir, None)
    }

    /// [`open`](Self::open) with a say in the engine (ADR-018): a fresh
    /// directory is created for `engine`; an existing one must already be
    /// `engine`, and the mismatch is refused by name rather than quietly
    /// opened as whatever it is — that is how a user ends up on the wrong
    /// engine without knowing. `None` means no preference.
    pub fn open_with_engine(
        data_dir: &Path,
        engine: Option<StorageEngine>,
    ) -> Result<Self, PortError> {
        let opened = match engine {
            Some(engine) => EmbeddedKernelStore::open_with_engine(data_dir, engine),
            None => EmbeddedKernelStore::open(data_dir),
        };
        let store = opened?;

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
        let (quality_observer, telemetry_guard, telemetry_writer, quality_telemetry_error) =
            compose_quality_telemetry(data_dir);

        // Read back rather than trusted from the request: what the directory
        // says it is, is what we are on.
        let engine = EmbeddedKernelStore::engine_of(data_dir)?;

        Ok(Self {
            data_dir: data_dir.to_path_buf(),
            engine,
            store,
            service,
            quality_observer,
            telemetry_guard,
            telemetry_writer,
            quality_telemetry_error,
        })
    }

    /// The engine behind this kernel's store, as its data directory records
    /// it. Surfaced at startup and by the doctor so a user can see which one
    /// they are on.
    pub fn engine(&self) -> StorageEngine {
        self.engine
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

    pub fn quality_observer(&self) -> Arc<dyn QualityMetricsObserver> {
        self.quality_observer.clone()
    }

    pub fn quality_telemetry_dropped_observations(&self) -> u64 {
        self.quality_observer.dropped_observations()
    }

    pub fn quality_telemetry_write_failures(&self) -> u64 {
        self.telemetry_writer
            .as_ref()
            .map_or(0, |writer| writer.write_failures())
    }

    pub fn quality_telemetry_error(&self) -> Option<&str> {
        self.quality_telemetry_error.as_deref()
    }

    pub fn quality_telemetry_active(&self) -> bool {
        self.telemetry_guard.is_some()
    }

    /// Live query side for the same shareable SQLite journal used by the
    /// quality observer.
    pub fn quality_telemetry_reader(&self) -> Option<SqliteQualityTelemetryReader> {
        self.telemetry_writer.as_ref().map(|writer| writer.reader())
    }
}

fn compose_quality_telemetry(
    data_dir: &Path,
) -> (
    Arc<BufferedQualityMetricsObserver>,
    Option<EmbeddedTelemetryGuard>,
    Option<Arc<SqliteQualityTelemetryWriter>>,
    Option<String>,
) {
    let (observer, receiver) = BufferedQualityMetricsObserver::with_capacity(1_024);
    let observer = Arc::new(observer);
    let writer =
        match SqliteQualityTelemetryWriter::open(data_dir, QualityTelemetryRetention::default()) {
            Ok(writer) => Arc::new(writer),
            Err(error) => {
                drop(receiver);
                let raw = error.to_string();
                let reason = if raw.contains("Cannot acquire lock")
                    || raw.to_ascii_lowercase().contains("already open")
                {
                    format!("the store's quality telemetry is held by another process ({raw})")
                } else {
                    raw
                };
                return (observer, None, None, Some(reason));
            }
        };
    let batch_writer = Arc::clone(&writer);
    let final_writer = Arc::clone(&writer);
    let guard = match EmbeddedTelemetryGuard::try_spawn(
        receiver,
        64,
        Duration::from_millis(250),
        move |batch| {
            let _ = batch_writer.write_batch(&batch);
        },
        move || {
            let _ = final_writer.flush_durable();
        },
    ) {
        Ok(guard) => guard,
        Err(error) => {
            return (
                observer,
                None,
                Some(writer),
                Some(format!("quality telemetry worker could not start: {error}")),
            );
        }
    };
    (observer, Some(guard), Some(writer), None)
}

#[cfg(test)]
mod tests {
    use kmp_domain::{BundleQualityMetrics, QualityMetricsObserver, QualityObservationContext};

    use super::EmbeddedKernel;

    #[test]
    fn telemetry_startup_failure_never_prevents_the_kernel_from_opening() {
        let data_dir = tempfile::tempdir().expect("temp data dir");
        std::fs::write(
            data_dir.path().join("telemetry"),
            b"blocks directory creation",
        )
        .expect("blocking file");

        let kernel = EmbeddedKernel::open(data_dir.path()).expect("kernel remains available");
        assert!(!kernel.quality_telemetry_active());
        assert!(kernel.quality_telemetry_error().is_some());
        let metrics = BundleQualityMetrics::new(1, 1.0, 0.0, 0.0, 0.0).expect("valid metrics");
        kernel.quality_observer.observe(
            &metrics,
            &QualityObservationContext {
                rpc: "kmp_wake".to_string(),
                root_node_id: "question:fail-open".to_string(),
                role: "resumer".to_string(),
                revision: Some(1),
            },
        );
        assert_eq!(kernel.quality_telemetry_dropped_observations(), 1);
        assert_eq!(kernel.quality_telemetry_write_failures(), 0);
    }

    #[test]
    fn live_quality_reader_shares_the_kernel_journal() {
        let data_dir = tempfile::tempdir().expect("temp data dir");
        let kernel = EmbeddedKernel::open(data_dir.path()).expect("kernel");
        let reader = kernel
            .quality_telemetry_reader()
            .expect("active telemetry exposes its read side");

        assert_eq!(reader.count().expect("shared journal is readable"), 0);
    }

    #[test]
    fn two_telemetry_compositions_share_the_sqlite_journal() {
        let data_dir = tempfile::tempdir().expect("temp data dir");
        let first = super::compose_quality_telemetry(data_dir.path());
        assert!(first.2.is_some(), "the first writer opens the journal");
        let second = super::compose_quality_telemetry(data_dir.path());
        assert!(
            second.2.is_some(),
            "the second writer opens the same journal"
        );
        assert!(
            second.3.is_none(),
            "SQLite does not report process ownership"
        );
    }
}
