use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::error::BoxError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperMetricRelationship {
    pub source_node_id: String,
    pub target_node_id: String,
    pub relationship_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperUseCaseMetric {
    pub use_case_id: String,
    pub variant_id: String,
    pub variant_label: String,
    pub relation_variant: String,
    pub detail_variant: String,
    pub graph_scale: String,
    pub requested_token_budget: u32,
    pub title: String,
    pub question: String,
    pub root_node_id: String,
    pub target_node_id: String,
    pub bundle_nodes: u32,
    pub bundle_relationships: u32,
    pub detailed_nodes: u32,
    pub rendered_token_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_latency_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_latency_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_load_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail_load_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_assembly_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail_batch_size: Option<u32>,
    pub explanation_roundtrip_fidelity: f64,
    pub detail_roundtrip_fidelity: f64,
    pub causal_reconstruction_score: f64,
    pub rendered_contains_expected_rationale: bool,
    pub rendered_contains_expected_decision_reference: bool,
    pub rendered_contains_expected_detail: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rehydration_point_hit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rehydration_node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_success_hit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_success_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_target_node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dominant_reason_hit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suspect_relationship_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_graph_relationship_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motivation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caused_by_node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suspect_relationships: Vec<PaperMetricRelationship>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_task_success: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_restart_accuracy: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_restart_exact: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_restart_off_by_one: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_restart_on_competing: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_restart_explained: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_reason_preserved: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_reason_correct: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_reason_distractor: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_latency_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_judge_raw: Option<String>,
}

pub fn ratio(hits: usize, total: usize) -> f64 {
    if total == 0 {
        return 0.0;
    }

    hits as f64 / total as f64
}

pub fn emit_metric(metric: &PaperUseCaseMetric) -> Result<(), BoxError> {
    let Some(metrics_dir) = env::var_os("KMP_PAPER_METRICS_DIR") else {
        return Ok(());
    };

    let _guard = metric_write_lock()
        .lock()
        .expect("paper metric write lock should not be poisoned");

    let metrics_dir = PathBuf::from(metrics_dir);
    fs::create_dir_all(&metrics_dir)?;

    let metric_path = metrics_dir.join(format!("{}.json", metric_file_stem(metric)));
    fs::write(&metric_path, serde_json::to_vec_pretty(metric)?)?;

    if let Some(summary_path) = env::var_os("KMP_PAPER_SUMMARY_PATH") {
        write_summary(&metrics_dir, &PathBuf::from(summary_path))?;
    }

    Ok(())
}

fn write_summary(metrics_dir: &Path, summary_path: &Path) -> Result<(), BoxError> {
    let mut metrics = fs::read_dir(metrics_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .filter(|path| path != summary_path)
        .map(|path| fs::read(&path))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter_map(|bytes| serde_json::from_slice::<PaperUseCaseMetric>(&bytes).ok())
        .collect::<Vec<_>>();

    metrics.sort_by(|left, right| {
        left.use_case_id
            .cmp(&right.use_case_id)
            .then_with(|| left.variant_id.cmp(&right.variant_id))
    });
    fs::write(summary_path, serde_json::to_vec_pretty(&metrics)?)?;

    Ok(())
}

fn metric_write_lock() -> &'static Mutex<()> {
    static METRIC_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    METRIC_WRITE_LOCK.get_or_init(|| Mutex::new(()))
}

fn metric_file_stem(metric: &PaperUseCaseMetric) -> String {
    if metric.variant_id == "full_explanatory_with_detail" {
        metric.use_case_id.clone()
    } else {
        format!("{}__{}", metric.use_case_id, metric.variant_id)
    }
}
