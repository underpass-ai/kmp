//! Every optional file beside a store, read once, and the acknowledgement
//! of which took effect. The server opens a store through this, and
//! `doctor` and `info` read the same acknowledgement through
//! [`inspect_store_config`], so the report cannot drift from what a session
//! actually applies (#887).

use std::path::Path;
use std::sync::Arc;

use kmp_proto_mapping::v1beta1::{AskGate, LexicalBridge};

use super::ask_gate_config::{ASK_GATE_FILE, AskGateConfig};
use super::curate_config::{CURATE_FILE, CurateConfig};
use super::doubt_band_judge::DoubtBandJudge;
use super::judgement_reranker::JudgementReranker;
use super::judgement_source::load_judgement;
use super::lexical_bridge_file::{lexical_bridge_path, load_lexical_bridge};
use super::lexical_index::index_limits::IndexLimits;
use super::lexical_index_config::{LEXICAL_INDEX_CONFIG_FILE, LexicalIndexConfig};
use super::loopback_semantic_retriever::LoopbackSemanticRetriever;
use super::observed_judgement::ObservedJudgement;
use super::store_config_report::StoreConfigReport;
use super::store_file_verdict::StoreFileVerdict;
use super::verdict_book_access::VerdictBookAccess;
use super::verdict_book_config::{VERDICT_BOOK_CONFIG_FILE, VerdictBookConfig};
use super::verdict_ledger::VerdictLedger;
use super::wake_focus_judge::WakeFocusJudge;
use super::write_expansions_config::{WRITE_EXPANSIONS_FILE, WriteExpansionsConfig};
use super::write_relations_config::WriteRelationsConfig;
use crate::curate::domain::partner_cap::PartnerCap;
use crate::curate::domain::partner_filter::PartnerFilter;
use crate::curate::domain::paths_corridor::PathsCorridor;
use crate::serving::environment::{
    TYPESAFE_API_KEY_ENV, TYPESAFE_CASSETTE_ENV, TYPESAFE_CASSETTE_MODE_ENV, optional_env_string,
};
use crate::serving::judgement_site::JudgementSite;
use crate::serving::ports::judgement_model::JudgementModel;
use crate::serving::ports::semantic_candidate_provider::SemanticCandidateProvider;
use crate::serving::store_config_entry::StoreConfigEntry;

/// Which files a store opened with, as loaded values.
pub(super) struct StoreConfigLoads {
    pub(super) judgement: Result<Option<Arc<dyn JudgementModel>>, String>,
    pub(super) ledger: Result<Option<Arc<VerdictLedger>>, String>,
    pub(super) rerank: Result<Option<Arc<JudgementReranker>>, String>,
    pub(super) wake_focus: Result<Option<Arc<WakeFocusJudge>>, String>,
    pub(super) doubt_band: Result<Option<Arc<DoubtBandJudge>>, String>,
    pub(super) write_relations: Option<WriteRelationsConfig>,
    pub(super) write_expansions: Result<Option<WriteExpansionsConfig>, String>,
    pub(super) lexical_bridge: LexicalBridge,
    pub(super) semantic: Result<Option<Arc<dyn SemanticCandidateProvider>>, String>,
    pub(super) ask_gate: Result<Option<AskGate>, String>,
    pub(super) curate: Result<(PartnerCap, PartnerFilter, PathsCorridor), String>,
    pub(super) index_limits: Result<IndexLimits, String>,
}

impl StoreConfigLoads {
    pub(super) fn load(data_dir: &Path, book: VerdictBookAccess) -> Self {
        let judgement = load_judgement(
            data_dir,
            optional_env_string(TYPESAFE_API_KEY_ENV),
            optional_env_string(TYPESAFE_CASSETTE_ENV),
            optional_env_string(TYPESAFE_CASSETTE_MODE_ENV),
        );
        let ledger = match book {
            VerdictBookAccess::Open => VerdictLedger::load(
                data_dir,
                &judgement,
                optional_env_string(TYPESAFE_CASSETTE_ENV).is_some(),
            ),
            VerdictBookAccess::ConfigurationOnly => VerdictBookConfig::load(data_dir).map(|_| None),
        };
        let observed = |site| {
            let book = ledger.as_ref().ok().and_then(Option::as_ref);
            ObservedJudgement::for_site(&judgement, book, site)
        };
        let rerank = JudgementReranker::load(data_dir, &observed(JudgementSite::Rerank));
        let wake_focus = WakeFocusJudge::load(data_dir, &observed(JudgementSite::WakeFocus));
        let doubt_band = DoubtBandJudge::load(data_dir, &observed(JudgementSite::DoubtBand));
        Self {
            rerank,
            wake_focus,
            doubt_band,
            write_relations: WriteRelationsConfig::load(data_dir),
            write_expansions: WriteExpansionsConfig::load(data_dir),
            lexical_bridge: load_lexical_bridge(data_dir),
            semantic: LoopbackSemanticRetriever::load(data_dir),
            ask_gate: AskGateConfig::load(data_dir),
            curate: CurateConfig::load(data_dir),
            index_limits: LexicalIndexConfig::load(data_dir),
            judgement,
            ledger,
        }
    }

    /// Which of the store's optional files took effect, why the others did
    /// not, and the values that matter of those that did.
    pub(super) fn report(&self, data_dir: &Path) -> StoreConfigReport {
        let judged = verdict(&self.judgement);
        let needs_jev = |file: &str| {
            judged
                .clone()
                .map_err(|error| format!("{file} needs a working typesafe.json: {error}"))
        };
        let mut report = StoreConfigReport::new(data_dir)
            .beside_store("typesafe.json", judged.clone())
            .at(
                "typesafe-cassette",
                optional_env_string(TYPESAFE_CASSETTE_ENV).map(Into::into),
                judged.clone(),
            )
            .beside_store("rerank.json", verdict(&self.rerank))
            .beside_store("wake-focus.json", verdict(&self.wake_focus))
            .beside_store("semantic-retrieval.json", verdict(&self.semantic))
            .beside_store(
                WriteRelationsConfig::FILE,
                write_relations_verdict(self.write_relations.as_ref(), &self.judgement),
            )
            .beside_store(
                VERDICT_BOOK_CONFIG_FILE,
                self.ledger.as_ref().map(|_| ()).map_err(Clone::clone),
            )
            .beside_store(
                ASK_GATE_FILE,
                self.ask_gate.as_ref().map(|_| ()).map_err(Clone::clone),
            )
            .beside_store(
                CURATE_FILE,
                self.curate.as_ref().map(|_| ()).map_err(Clone::clone),
            )
            .beside_store(
                LEXICAL_INDEX_CONFIG_FILE,
                self.index_limits.as_ref().map(|_| ()).map_err(Clone::clone),
            )
            .beside_store(
                WRITE_EXPANSIONS_FILE,
                match &self.write_expansions {
                    Ok(Some(_)) => needs_jev("write-expansions.json"),
                    Ok(None) => Err("not loaded".into()),
                    Err(error) => Err(error.clone()),
                },
            )
            .beside_store("ask-judge.json", verdict(&self.doubt_band))
            .at(
                "lexical-bridge.kmpb",
                lexical_bridge_path(data_dir),
                if self.lexical_bridge.is_silent() {
                    Err("the table is unreadable or empty".into())
                } else {
                    Ok(())
                },
            );
        if let Ok(Some(model)) = &self.judgement {
            report = report.effective("typesafe.json", format!("model {}", model.model()));
        }
        if let Ok(Some(rerank)) = &self.rerank {
            report = report.effective("rerank.json", format!("pool {}", rerank.pool_size()));
            report = report.effective(
                "rerank.json",
                match rerank.margin_tenths() {
                    Some(tenths) => format!("margin {}", tenths_text(tenths)),
                    None => "margin off".into(),
                },
            );
        }
        if let Ok(Some(judge)) = &self.doubt_band {
            report = report
                .effective(
                    "ask-judge.json",
                    format!("veto at {}", permille_text(judge.veto_permille())),
                )
                .effective(
                    "ask-judge.json",
                    format!("margin {}", tenths_text(judge.margin_tenths())),
                )
                .effective(
                    "ask-judge.json",
                    match judge.promote_permille() {
                        Some(permille) => format!("promote at {}", permille_text(permille)),
                        None => "promote off".into(),
                    },
                );
        }
        if let Ok((cap, filter, corridor)) = &self.curate {
            report = report
                .effective(CURATE_FILE, format!("partner facts {}", cap.facts()))
                .effective(CURATE_FILE, format!("partner filter {}", filter.name()))
                .effective(CURATE_FILE, format!("paths corridor {}", corridor.name()));
        }
        if let Some(config) = &self.write_relations {
            report = report.effective(
                WriteRelationsConfig::FILE,
                format!("lifecycle {}", config.lifecycle().name()),
            );
        }
        report
    }
}

/// The acknowledgement a session would log for the store at `data_dir`,
/// read without opening the store or its verdict book.
pub(crate) fn inspect_store_config(data_dir: &Path) -> Vec<StoreConfigEntry> {
    StoreConfigLoads::load(data_dir, VerdictBookAccess::ConfigurationOnly)
        .report(data_dir)
        .entries()
}

fn verdict<T>(loaded: &Result<Option<T>, String>) -> Result<(), String> {
    match loaded {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err("not loaded".into()),
        Err(error) => Err(error.clone()),
    }
}

/// `write-relations.json` is the opt-in whatever it says, so a file that is
/// not understood still applies, with a warning naming what was not (#886).
pub(super) fn write_relations_verdict<A>(
    config: Option<&WriteRelationsConfig>,
    judgement: &Result<Option<A>, String>,
) -> StoreFileVerdict {
    match (judgement, config.and_then(WriteRelationsConfig::warning)) {
        (Err(error), _) => StoreFileVerdict::Ignored(format!(
            "write relations need a working typesafe.json: {error}"
        )),
        (Ok(None), _) => StoreFileVerdict::Ignored(
            "write relations need a working typesafe.json: not loaded".into(),
        ),
        (Ok(Some(_)), Some(warning)) => StoreFileVerdict::AppliedWithWarning(warning.to_string()),
        (Ok(Some(_)), None) => StoreFileVerdict::Applied,
    }
}

fn tenths_text(tenths: i64) -> String {
    format!("{:.1}", tenths as f64 / 10.0)
}

fn permille_text(permille: u16) -> String {
    format!("{:.3}", f64::from(permille) / 1000.0)
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}
