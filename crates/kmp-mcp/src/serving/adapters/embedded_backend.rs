use super::ask_gate_config::{ASK_GATE_FILE, AskGateConfig};
use super::curate_config::{CURATE_FILE, CurateConfig};
use super::curate_doubt_cache::CurateDoubtCache;
use super::curate_review_cache::CurateReviewCache;
use super::doubt_band_judge::DoubtBandJudge;
use super::embedded::{
    EmbeddedAskTool, EmbeddedCondenseTool, EmbeddedCurateTool, EmbeddedIngestTool,
    EmbeddedInspectTool, EmbeddedNearTool, EmbeddedReadTelemetry, EmbeddedRelabelTool,
    EmbeddedRelateTool, EmbeddedSummariesAuditTool, EmbeddedTemporalMoveTool, EmbeddedTraceTool,
    EmbeddedVisualProjectionTool, EmbeddedWakeTool, FrozenRecallReads,
};
use super::judgement_reranker::JudgementReranker;
use super::judgement_source::load_judgement;
use super::lexical_bridge_file::{lexical_bridge_path, load_lexical_bridge};
use super::loopback_semantic_retriever::LoopbackSemanticRetriever;
use super::observed_judgement::ObservedJudgement;
use super::process_frozen_recalls::ProcessFrozenRecalls;
use super::store_config_report::StoreConfigReport;
use super::verdict_book_config::VERDICT_BOOK_CONFIG_FILE;
use super::verdict_ledger::VerdictLedger;
use super::wake_focus_judge::WakeFocusJudge;
use super::write_relations_config::WriteRelationsConfig;
use crate::contract::{TIME_TOOL, TimeMove};
use crate::curate::domain::lifecycle_mode::LifecycleMode;
use crate::curate::domain::partner_cap::PartnerCap;
use crate::curate::domain::partner_filter::PartnerFilter;
use crate::serving::environment::{
    EVAL_PARTNER_FACTS_ENV, TYPESAFE_API_KEY_ENV, TYPESAFE_CASSETTE_ENV,
    TYPESAFE_CASSETTE_MODE_ENV, optional_env_string,
};
use crate::serving::judgement_site::JudgementSite;
use crate::serving::ports::judgement_model::JudgementModel;
use crate::serving::ports::semantic_candidate_provider::SemanticCandidateProvider;
use crate::serving::tool_success_result;
use crate::serving::{KernelMcpToolBackend, KernelMcpToolFuture, ToolError};
use kmp_domain::TemporalDirection;
use kmp_embedded::{CommitNativeBundle, EmbeddedKernel};
use kmp_proto_mapping::v1beta1::{AskGate, LexicalBridge, LexicalIndexCache};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;

/// In-process kernel backend: the same JSON argument builders and response
/// shapes as live mode, with the application service called directly instead
/// of a gRPC channel — identical tool JSON by construction.
pub struct EmbeddedKernelMcpBackend {
    kernel: EmbeddedKernel,
    data_dir: String,
    commit_native: Option<CommitNativeBundle>,
    /// The word table `ask` bridges languages with, read once beside the
    /// store. Silent when none is installed.
    lexical_bridge: LexicalBridge,
    lexical_cache: Arc<LexicalIndexCache>,
    /// First pages of paged Wake and Ask reads, kept so their continuations
    /// cut pages instead of reading again while the store stands still.
    frozen_recalls: ProcessFrozenRecalls,
    semantic: Result<Option<Arc<dyn SemanticCandidateProvider>>, String>,
    /// TypeSafe Jev for `kmp_curate`, opted into per store. Off without
    /// `typesafe.json`; an error names why a present opt-in cannot run.
    judgement: Result<Option<Arc<dyn JudgementModel>>, String>,
    /// Verdicts already judged, beside the store (`judgements.sqlite3`):
    /// only with a working `typesafe.json`, unless `judgement-book.json`
    /// turns it off.
    ledger: Option<Arc<VerdictLedger>>,
    /// Ask re-ranking by the same model, opted into separately because it
    /// sends text on every Ask.
    rerank: Result<Option<Arc<JudgementReranker>>, String>,
    /// Wake focused on its intent by the same model, its own opt-in.
    wake_focus: Result<Option<Arc<WakeFocusJudge>>, String>,
    /// The ask doubt band judged by the same model (`ask-judge.json`).
    doubt_band: Result<Option<Arc<DoubtBandJudge>>, String>,
    /// Relations proposed after each write, opted into by
    /// `write-relations.json` beside `typesafe.json`.
    write_relations: bool,
    /// Whether and how focused reviews propose write-time lifecycle pairs
    /// (`write-relations.json` `lifecycle`).
    lifecycle: LifecycleMode,
    /// The largest about a review without `focus` pairs orphans in:
    /// `curate.json` `partner_facts`, the default without it, or what an
    /// evaluation names.
    partner_cap: PartnerCap,
    /// What the pairs of that partner round must pass (`curate.json`
    /// `partner_filter`, off by default).
    partner_filter: PartnerFilter,
    /// The anchored ask gate: [`AskGate::STORE_DEFAULT`] (on) unless
    /// `ask-gate.json` beside the store says otherwise; a file that cannot
    /// apply is reported and the default stands.
    ask_gate: Option<AskGate>,
    curate_reviews: CurateReviewCache,
    curate_doubts: CurateDoubtCache,
}

impl EmbeddedKernelMcpBackend {
    pub fn open(data_dir: &Path) -> Result<Self, String> {
        Self::open_with_engine(
            data_dir,
            kmp_embedded::default_engine_for_data_dir(data_dir),
        )
    }

    /// `engine` is what a fresh directory gets; an existing one must agree
    /// (ADR-018). `None` defers to the directory, or the default.
    pub fn open_with_engine(
        data_dir: &Path,
        engine: Option<kmp_embedded::StorageEngine>,
    ) -> Result<Self, String> {
        Self::open_with_engine_and_commit_native(data_dir, engine, None)
    }

    pub fn open_with_engine_and_commit_native(
        data_dir: &Path,
        engine: Option<kmp_embedded::StorageEngine>,
        commit_native: Option<CommitNativeBundle>,
    ) -> Result<Self, String> {
        let kernel = EmbeddedKernel::open_with_engine(data_dir, engine)
            .map_err(|error| error.to_string())?;
        let judgement = load_judgement(
            data_dir,
            optional_env_string(TYPESAFE_API_KEY_ENV),
            optional_env_string(TYPESAFE_CASSETTE_ENV),
            optional_env_string(TYPESAFE_CASSETTE_MODE_ENV),
        );
        let ledger = VerdictLedger::load(
            data_dir,
            &judgement,
            optional_env_string(TYPESAFE_CASSETTE_ENV).is_some(),
        );
        let book = ledger.as_ref().ok().and_then(Option::as_ref);
        let rerank = JudgementReranker::load(
            data_dir,
            &ObservedJudgement::for_site(&judgement, book, JudgementSite::Rerank),
        );
        let wake_focus = WakeFocusJudge::load(
            data_dir,
            &ObservedJudgement::for_site(&judgement, book, JudgementSite::WakeFocus),
        );
        let doubt_band = DoubtBandJudge::load(
            data_dir,
            &ObservedJudgement::for_site(&judgement, book, JudgementSite::DoubtBand),
        );
        let write_relations = WriteRelationsConfig::load(data_dir);
        let lifecycle = write_relations
            .as_ref()
            .map(WriteRelationsConfig::lifecycle)
            .unwrap_or_default();
        let write_relations = write_relations.is_some();
        let lexical_bridge = load_lexical_bridge(data_dir);
        let semantic = LoopbackSemanticRetriever::load(data_dir);
        let ask_gate = AskGateConfig::load(data_dir);
        let curate = CurateConfig::load(data_dir);
        acknowledge_store_config(data_dir, &judgement, &rerank, &wake_focus, &semantic)
            .beside_store(
                VERDICT_BOOK_CONFIG_FILE,
                ledger.as_ref().map(|_| ()).map_err(Clone::clone),
            )
            .beside_store(
                ASK_GATE_FILE,
                ask_gate.as_ref().map(|_| ()).map_err(Clone::clone),
            )
            .beside_store(
                CURATE_FILE,
                curate.as_ref().map(|_| ()).map_err(Clone::clone),
            )
            .beside_store(
                "ask-judge.json",
                match &doubt_band {
                    Ok(Some(_)) => Ok(()),
                    Ok(None) => Err("not loaded".into()),
                    Err(error) => Err(error.clone()),
                },
            )
            .at(
                "lexical-bridge.kmpb",
                lexical_bridge_path(data_dir),
                if lexical_bridge.is_silent() {
                    Err("the table is unreadable or empty".into())
                } else {
                    Ok(())
                },
            )
            .emit();
        let (partner_cap, partner_filter) =
            curate.unwrap_or((PartnerCap::DEFAULT, PartnerFilter::Off));
        Ok(Self {
            kernel,
            data_dir: data_dir.display().to_string(),
            commit_native,
            lexical_bridge,
            lexical_cache: Arc::default(),
            frozen_recalls: ProcessFrozenRecalls::default(),
            semantic,
            judgement,
            ledger: ledger.ok().flatten(),
            rerank,
            wake_focus,
            doubt_band,
            write_relations,
            lifecycle,
            partner_cap: optional_env_string(EVAL_PARTNER_FACTS_ENV)
                .as_deref()
                .and_then(PartnerCap::named)
                .unwrap_or(partner_cap),
            partner_filter,
            ask_gate: ask_gate.unwrap_or(AskGate::STORE_DEFAULT),
            curate_reviews: CurateReviewCache::default(),
            curate_doubts: CurateDoubtCache::default(),
        })
    }

    /// The store's judgement observed for one call site, when it can run.
    fn observed(&self, site: JudgementSite) -> Option<ObservedJudgement> {
        match &self.judgement {
            Ok(Some(model)) => Some(ObservedJudgement::at_site(
                model,
                self.ledger.as_ref(),
                site,
            )),
            _ => None,
        }
    }

    /// The table `ask` bridges languages with on this store.
    pub fn lexical_bridge(&self) -> &LexicalBridge {
        &self.lexical_bridge
    }

    /// The storage engine this session's store is on.
    pub fn engine(&self) -> kmp_embedded::StorageEngine {
        self.kernel.engine()
    }

    pub fn data_dir(&self) -> &str {
        &self.data_dir
    }

    /// The opened kernel, for composition roots that mount additional
    /// in-process surfaces (the viewer) over this same session's store.
    pub fn kernel(&self) -> &EmbeddedKernel {
        &self.kernel
    }
}

/// Which of the store's optional files took effect, and why the others did
/// not; the lexical bridge is added by the caller that loaded it.
fn acknowledge_store_config<A, B, C>(
    data_dir: &Path,
    judgement: &Result<Option<A>, String>,
    rerank: &Result<Option<B>, String>,
    wake_focus: &Result<Option<C>, String>,
    semantic: &Result<Option<Arc<dyn SemanticCandidateProvider>>, String>,
) -> StoreConfigReport {
    fn verdict<T>(loaded: &Result<Option<T>, String>) -> Result<(), String> {
        match loaded {
            Ok(Some(_)) => Ok(()),
            Ok(None) => Err("not loaded".into()),
            Err(error) => Err(error.clone()),
        }
    }
    let judged = verdict(judgement);
    StoreConfigReport::new(data_dir)
        .beside_store("typesafe.json", judged.clone())
        .at(
            "typesafe-cassette",
            optional_env_string(TYPESAFE_CASSETTE_ENV).map(Into::into),
            judged.clone(),
        )
        .beside_store("rerank.json", verdict(rerank))
        .beside_store("wake-focus.json", verdict(wake_focus))
        .beside_store(
            WriteRelationsConfig::FILE,
            judged
                .map_err(|error| format!("write relations need a working typesafe.json: {error}")),
        )
        .beside_store("semantic-retrieval.json", verdict(semantic))
}

impl KernelMcpToolBackend for EmbeddedKernelMcpBackend {
    fn backend_name(&self) -> &'static str {
        "embedded"
    }

    fn bridges_languages(&self) -> bool {
        !self.lexical_bridge.is_silent()
    }

    fn call_tool<'a>(&'a self, name: &'a str, arguments: &'a Value) -> KernelMcpToolFuture<'a> {
        let service = self.kernel.service();
        let observer = self.kernel.quality_observer();
        Box::pin(async move {
            let telemetry = EmbeddedReadTelemetry::new(observer.as_ref());
            match name {
                "kmp_ingest" | "kernel_remember" | "kernel_ingest_context" => {
                    EmbeddedIngestTool::new(
                        &service,
                        self.commit_native.as_ref(),
                        self.kernel.store(),
                    )
                    .call(arguments)
                    .await
                }
                "kmp_wake" => {
                    EmbeddedWakeTool::new(
                        &service,
                        telemetry,
                        &self.wake_focus,
                        FrozenRecallReads::new(&self.frozen_recalls, &service),
                    )
                    .call(arguments)
                    .await
                }
                "kmp_ask" => {
                    EmbeddedAskTool::new(
                        &service,
                        telemetry,
                        &self.lexical_bridge,
                        &self.semantic,
                        &self.rerank,
                        &self.lexical_cache,
                        FrozenRecallReads::new(&self.frozen_recalls, &service),
                    )
                    .with_gate(self.ask_gate)
                    .with_doubt_band(&self.doubt_band)
                    .call(arguments)
                    .await
                }
                TIME_TOOL => {
                    let (direction, name) = match TimeMove::from_arguments(arguments)? {
                        TimeMove::Near => {
                            return EmbeddedNearTool::new(&service).call(arguments).await;
                        }
                        TimeMove::Goto => (TemporalDirection::Goto, "goto"),
                        TimeMove::Rewind => (TemporalDirection::Rewind, "rewind"),
                        TimeMove::Forward => (TemporalDirection::Forward, "forward"),
                    };
                    EmbeddedTemporalMoveTool::new(&service, direction, name)
                        .call(arguments)
                        .await
                }
                "kmp_relate" => {
                    EmbeddedRelateTool::new(&service, telemetry, &self.lexical_bridge)
                        .call(arguments)
                        .await
                }
                "kmp_curate" => {
                    let observed = self.observed(JudgementSite::of_curate(arguments));
                    let (judgement, warning) = match (&observed, &self.judgement) {
                        (Some(model), _) => (Some(model as &dyn JudgementModel), None),
                        (None, Err(error)) => (None, Some(error.as_str())),
                        (None, Ok(_)) => (None, None),
                    };
                    // Internal: the write dispatcher asks for the relations
                    // the memories it just wrote are missing. Off unless the
                    // store opted in and Jev can run.
                    let mut arguments = arguments.clone();
                    if arguments.get("mode").and_then(Value::as_str) == Some("write_proposals") {
                        if !self.write_relations || judgement.is_none() {
                            return Ok(tool_success_result(json!({"enabled": false})));
                        }
                        arguments["mode"] = json!("review");
                    }
                    let arguments = &arguments;
                    EmbeddedCurateTool::new(
                        &service,
                        telemetry,
                        &self.lexical_bridge,
                        judgement,
                        warning,
                        &self.curate_reviews,
                        &self.curate_doubts,
                    )
                    .with_lifecycle(self.lifecycle)
                    .with_partners(self.partner_cap, self.partner_filter)
                    .call(arguments)
                    .await
                }
                "kmp_trace" => {
                    EmbeddedTraceTool::new(&service, telemetry)
                        .call(arguments)
                        .await
                }
                "kmp_inspect" => EmbeddedInspectTool::new(&service).call(arguments).await,
                "kmp_relabel" => EmbeddedRelabelTool::new(&service).call(arguments).await,
                "kmp_condense" => {
                    EmbeddedCondenseTool::new(
                        &service,
                        self.commit_native.as_ref(),
                        self.kernel.store(),
                    )
                    .call(arguments)
                    .await
                }
                // The one read that is made off the event log rather than
                // the projections: a summary's earlier revisions are what
                // say whether the text moved after it was written.
                "kmp_summaries_audit" => {
                    let observed = self.observed(JudgementSite::Summaries);
                    let judgement = observed.as_ref().map(|model| model as &dyn JudgementModel);
                    EmbeddedSummariesAuditTool::new(self.kernel.store(), judgement)
                        .call(arguments)
                        .await
                }
                "kmp_view_read_nodes" => {
                    super::embedded::EmbeddedMemoryNodesTool::new(&service)
                        .call(arguments)
                        .await
                }
                "kmp_view_read_projection" => {
                    EmbeddedVisualProjectionTool::new(&service)
                        .call(arguments)
                        .await
                }
                other => Err(ToolError::unknown_tool(format!(
                    "unknown KMP tool `{other}`"
                ))),
            }
        })
    }
}
