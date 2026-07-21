use std::future::Future;
use std::sync::Arc;

use rehydration_application::{
    CommandApplicationService, KernelMemoryApplicationService, ProjectionApplicationService,
    QueryApplicationService, RoutingProjectionWriter, UpdateContextUseCase,
};
use rehydration_domain::{
    ContextEventStore, GraphNeighborhoodReader, MemoryAboutIndexReader, NodeDetailReader,
    NodeRelationshipReader, ProcessedEventStore, ProjectionCheckpointStore, ProjectionWriter,
    SnapshotStore,
};

const CONFORMANCE_GENERATOR_VERSION: &str = "kmp-conformance";

pub type BackendProjectionWriter<G, D> = RoutingProjectionWriter<Arc<G>, Arc<D>>;
pub type BackendMemoryService<G, D, S, E> =
    KernelMemoryApplicationService<G, D, S, E, BackendProjectionWriter<G, D>>;
pub type BackendProjectionService<G, D, P, C> =
    ProjectionApplicationService<BackendProjectionWriter<G, D>, Arc<P>, Arc<C>>;

/// One isolated backend under test: the full port set a KMP kernel needs,
/// plus constructors for the application services composed over it.
pub struct ConformanceBackend<G, D, S, E, P, C> {
    pub graph: Arc<G>,
    pub detail: Arc<D>,
    pub snapshot: Arc<S>,
    pub events: Arc<E>,
    pub processed: Arc<P>,
    pub checkpoints: Arc<C>,
}

impl<G, D, S, E, P, C> ConformanceBackend<G, D, S, E, P, C>
where
    G: GraphNeighborhoodReader
        + MemoryAboutIndexReader
        + NodeRelationshipReader
        + ProjectionWriter
        + Send
        + Sync
        + 'static,
    D: NodeDetailReader + ProjectionWriter + Send + Sync + 'static,
    S: SnapshotStore + Send + Sync + 'static,
    E: ContextEventStore + Send + Sync + 'static,
    P: ProcessedEventStore + Send + Sync + 'static,
    C: ProjectionCheckpointStore + Send + Sync + 'static,
{
    pub fn new(graph: G, detail: D, snapshot: S, events: E, processed: P, checkpoints: C) -> Self {
        Self {
            graph: Arc::new(graph),
            detail: Arc::new(detail),
            snapshot: Arc::new(snapshot),
            events: Arc::new(events),
            processed: Arc::new(processed),
            checkpoints: Arc::new(checkpoints),
        }
    }

    /// The production write path: graph mutations routed to the graph store,
    /// detail mutations to the detail store.
    pub fn projection_writer(&self) -> BackendProjectionWriter<G, D> {
        RoutingProjectionWriter::new(Arc::clone(&self.graph), Arc::clone(&self.detail))
    }

    /// The KMP facade (`ingest`/`wake`/`ask`/`temporal`/`trace`/`inspect`)
    /// with synchronous in-process projection, mirroring the composition in
    /// `rehydration-transport-grpc::GrpcServer::new`.
    pub fn memory_service(&self) -> BackendMemoryService<G, D, S, E> {
        let query_application = Arc::new(QueryApplicationService::new(
            Arc::clone(&self.graph),
            Arc::clone(&self.detail),
            Arc::clone(&self.snapshot),
            CONFORMANCE_GENERATOR_VERSION,
        ));
        let update_context = Arc::new(UpdateContextUseCase::new_with_projection_writer(
            Arc::clone(&self.events),
            self.projection_writer(),
            CONFORMANCE_GENERATOR_VERSION,
        ));
        KernelMemoryApplicationService::new(
            query_application,
            Arc::new(CommandApplicationService::new(update_context)),
        )
    }

    /// The event-driven projection path (what the NATS runtime drives in the
    /// infrastructure edition), invoked in-process.
    pub fn projection_service(&self) -> BackendProjectionService<G, D, P, C> {
        ProjectionApplicationService::new(
            self.projection_writer(),
            Arc::clone(&self.processed),
            Arc::clone(&self.checkpoints),
        )
    }
}

/// The backend type a factory produces, spelled once.
pub type FactoryBackend<F> = ConformanceBackend<
    <F as ConformanceBackendFactory>::Graph,
    <F as ConformanceBackendFactory>::Detail,
    <F as ConformanceBackendFactory>::Snapshot,
    <F as ConformanceBackendFactory>::Events,
    <F as ConformanceBackendFactory>::Processed,
    <F as ConformanceBackendFactory>::Checkpoints,
>;

/// Builds a fresh, empty, isolated backend per scenario.
pub trait ConformanceBackendFactory: Send + Sync {
    type Graph: GraphNeighborhoodReader
        + MemoryAboutIndexReader
        + NodeRelationshipReader
        + ProjectionWriter
        + Send
        + Sync
        + 'static;
    type Detail: NodeDetailReader + ProjectionWriter + Send + Sync + 'static;
    type Snapshot: SnapshotStore + Send + Sync + 'static;
    type Events: ContextEventStore + Send + Sync + 'static;
    type Processed: ProcessedEventStore + Send + Sync + 'static;
    type Checkpoints: ProjectionCheckpointStore + Send + Sync + 'static;

    fn fresh(&self) -> impl Future<Output = FactoryBackend<Self>> + Send;
}
