use ::kmp_application::*;
use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_domain::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tokio::sync::Notify;

pub const ABOUT: &str = "project:proof-read";
pub const CLAIM: &str = "project:proof-read:entry:observation:claim";
const OTHER: &str = "project:proof-read:entry:observation:other";
const TIME: &str = "2026-09-10T10:00:00Z";

#[derive(Debug, Clone)]
pub struct Reads {
    pub store: EmbeddedKernelStore,
    pub pause: Arc<AtomicBool>,
    pub paused: Arc<Notify>,
    pub resume: Arc<Notify>,
    pub opened: Arc<AtomicUsize>,
    pub fail_snapshot: bool,
}
impl Reads {
    pub fn new(store: EmbeddedKernelStore) -> Self {
        Self {
            store,
            pause: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(Notify::new()),
            resume: Arc::new(Notify::new()),
            opened: Arc::new(AtomicUsize::new(0)),
            fail_snapshot: false,
        }
    }
    async fn after_graph(&self) {
        if self.pause.swap(false, Ordering::SeqCst) {
            self.paused.notify_one();
            self.resume.notified().await;
        }
    }
}
impl ReadSnapshotProvider<Reads, Reads> for Reads {
    fn open_snapshot(&self) -> ReadSnapshotFuture<'_, Reads, Reads> {
        Box::pin(async {
            self.opened.fetch_add(1, Ordering::SeqCst);
            if self.fail_snapshot {
                return Err(PortError::Unavailable("deliberate snapshot refusal".into()));
            }
            let mut frozen = self.clone();
            frozen.store = self.store.read_snapshot().await?;
            Ok((frozen.clone(), frozen))
        })
    }
}
impl GraphNeighborhoodReader for Reads {
    async fn load_nodes_batch(
        &self,
        ids: Vec<String>,
    ) -> Result<Vec<Option<NodeProjection>>, PortError> {
        let result = self.store.load_nodes_batch(ids).await;
        self.after_graph().await;
        result
    }

    async fn load_neighborhood(
        &self,
        root: &str,
        depth: u32,
    ) -> Result<Option<NodeNeighborhood>, PortError> {
        let result = self.store.load_neighborhood(root, depth).await;
        self.after_graph().await;
        result
    }
    async fn load_scoped_neighborhood(
        &self,
        request: &NeighborhoodRequest,
    ) -> Result<Option<NodeNeighborhood>, PortError> {
        let result = self.store.load_scoped_neighborhood(request).await;
        self.after_graph().await;
        result
    }
    async fn load_context_path(
        &self,
        root: &str,
        target: &str,
        depth: u32,
    ) -> Result<Option<ContextPathNeighborhood>, PortError> {
        let result = self.store.load_context_path(root, target, depth).await;
        self.after_graph().await;
        result
    }
}
impl NodeRelationshipReader for Reads {
    async fn load_node_relationships(
        &self,
        id: &str,
    ) -> Result<Option<NodeRelationships>, PortError> {
        self.store.load_node_relationships(id).await
    }
}
impl NodeDetailReader for Reads {
    async fn load_node_detail(&self, id: &str) -> Result<Option<NodeDetailProjection>, PortError> {
        self.store.load_node_detail(id).await
    }
    async fn load_node_details_batch(
        &self,
        ids: Vec<String>,
    ) -> Result<Vec<Option<NodeDetailProjection>>, PortError> {
        self.store.load_node_details_batch(ids).await
    }
}
impl MemoryAboutIndexReader for Reads {
    async fn list_memory_abouts(&self) -> Result<Vec<String>, PortError> {
        self.store.list_memory_abouts().await
    }
    async fn list_memory_abouts_by_dimensions<'a>(
        &'a self,
        ids: &'a [String],
    ) -> Result<Vec<String>, PortError> {
        self.store.list_memory_abouts_by_dimensions(ids).await
    }
}
pub type Service = KernelMemoryApplicationService<
    Reads,
    Reads,
    EmbeddedKernelStore,
    EmbeddedKernelStore,
    EmbeddedKernelStore,
>;
pub fn service(reads: Reads, snapshot: bool) -> Arc<Service> {
    let store = Arc::new(reads.store.clone());
    let reads = Arc::new(reads);
    let mut query =
        QueryApplicationService::new(reads.clone(), reads.clone(), store.clone(), "snapshot-test");
    if snapshot {
        query = query.with_read_snapshots(reads);
    }
    Arc::new(KernelMemoryApplicationService::new(
        Arc::new(query),
        Arc::new(CommandApplicationService::new(Arc::new(
            UpdateContextUseCase::new_with_projection_writer(
                store.clone(),
                store.as_ref().clone(),
                "snapshot-test",
            ),
        ))),
    ))
}
pub fn command(count: usize, version: &str) -> MemoryIngestCommand {
    let entries = [CLAIM, OTHER]
        .into_iter()
        .map(|id| MemoryEntryData {
            id: id.into(),
            kind: "observation".into(),
            text: format!("{version}: recorded fact {id}"),
            metadata: Default::default(),
            coordinates: vec![MemoryCoordinateData {
                dimension: "task".into(),
                scope_id: "task:proof".into(),
                occurred_at: Some(TIME.into()),
                observed_at: Some(TIME.into()),
                ingested_at: None,
                valid_from: None,
                valid_until: None,
                sequence: None,
                rank: None,
                metadata: Default::default(),
            }],
        })
        .collect();
    let evidence = (0..count)
        .map(|i| MemoryEvidenceData {
            id: format!("evidence:{ABOUT}:source-{i}"),
            supports: vec![CLAIM.into(), OTHER.into()],
            support_clocks: Default::default(),
            text: format!(
                "{version}: source {i}: {}",
                "The two observation records are documented in this source. ".repeat(8)
            ),
            source: Some(format!("fixture:S{i}")),
            time: Some(TIME.into()),
            metadata: Default::default(),
        })
        .collect();
    MemoryIngestCommand {
        about: ABOUT.into(),
        memory: MemoryData {
            dimensions: vec![MemoryDimensionData {
                id: "task:proof".into(),
                kind: "task".into(),
                title: None,
                metadata: Default::default(),
            }],
            entries,
            relations: vec![],
            evidence,
        },
        provenance: None,
        idempotency_key: format!("{count}-{version}"),
        dry_run: false,
        default_observation_to_ingestion: false,
        neighborhood_review: None,
        label_policy: Default::default(),
        receipt_context: None,
    }
}
pub fn inspect_query() -> InspectMemoryQuery {
    InspectMemoryQuery {
        about: ABOUT.into(),
        ref_id: CLAIM.into(),
        include_details: true,
        include_incoming: true,
        include_outgoing: true,
        include_raw: false,
    }
}
pub fn temporal_query() -> TemporalMemoryQuery {
    TemporalMemoryQuery {
        about: ABOUT.into(),
        direction: TemporalDirection::Goto,
        entry_selection: Some(
            TemporalEntrySelection::new(vec![CLAIM.into()]).expect("valid fixture selection"),
        ),
        axis: TemporalAxis::Observed,
        cursor: Some(TemporalCursor::Time(TIME.into())),
        interval: None,
        dimensions: DimensionSelection::default(),
        window: TemporalWindow::new(0, 0),
        limit_entries: Some(1),
        include: TemporalIncludeOptions {
            evidence: true,
            relations: true,
            raw_refs: false,
            dependencies: true,
        },
        token_budget: 1_000_000,
        depth: 3,
        max_tier: Some(ResolutionTier::L2EvidencePack),
    }
}

pub fn command_in(about: &str, version: &str) -> MemoryIngestCommand {
    let mut packet = command(2, version);
    packet.about = about.into();
    packet.idempotency_key = format!("{about}-{version}");
    for entry in &mut packet.memory.entries {
        entry.id = entry.id.replace(ABOUT, about);
        entry.text = entry.text.replace(ABOUT, about);
    }
    for source in &mut packet.memory.evidence {
        source.id = source.id.replace(ABOUT, about);
        source.text = source.text.replace(ABOUT, about);
        source.supports = source
            .supports
            .iter()
            .map(|id| id.replace(ABOUT, about))
            .collect();
    }
    packet
}
