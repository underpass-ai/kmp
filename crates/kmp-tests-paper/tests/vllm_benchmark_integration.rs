#![cfg(feature = "container-tests")]
use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use kmp_proto::v1beta1::{GetContextRequest, RehydrationMode, RenderedContext, ResolutionTier};
use kmp_testkit::{
    Domain, EvaluationGroundTruth, GeneratedSeed, GraphSeedConfig, LlmEvaluationResult,
    LlmEvaluatorConfig, LlmProvider, LlmSemanticClassifierMode, NoiseMode, RelationMix,
    calibrate_judge, evaluate_with_llm, generate_seed, seed_publisher::seed_to_projection_events,
};
use kmp_tests_shared::fixtures::TestFixture;
use kmp_tests_shared::ports::{ClosureSeed, SeedContext};
use serde::Serialize;
use tokio::time::sleep;

const SUBJECT_PREFIX: &str = "rehydration";
const BENCHMARK_ROLE: &str = "evaluator";

// ---------------------------------------------------------------------------
// Run directory — auto-created per execution with timestamp
// ---------------------------------------------------------------------------

struct RunDir {
    path: PathBuf,
    log: std::fs::File,
}

impl RunDir {
    fn create() -> Result<Self, Box<dyn Error + Send + Sync>> {
        let base = std::env::var("E2E_OUTPUT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../artifacts/e2e-runs")
            });
        let ts = chrono_stamp();
        let path = base.join(format!("vllm-{ts}"));
        fs::create_dir_all(&path)?;
        fs::create_dir_all(path.join("results"))?;
        let log = fs::File::create(path.join("test.log"))?;
        eprintln!("[RUN] output directory: {}", path.display());
        Ok(Self { path, log })
    }

    fn log(&mut self, msg: &str) {
        eprintln!("{msg}");
        let _ = writeln!(self.log, "{msg}");
    }

    fn write_result(
        &self,
        name: &str,
        result: &BenchmarkResult,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let json = serde_json::to_vec_pretty(result)?;
        fs::write(self.path.join("results").join(format!("{name}.json")), json)?;
        Ok(())
    }
}

fn chrono_stamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;
    let days = secs / 86400;
    let (year, month, day) = epoch_days_to_date(days);
    format!("{year:04}-{month:02}-{day:02}_{hours:02}{minutes:02}{seconds:02}")
}

fn epoch_days_to_date(days: u64) -> (u64, u64, u64) {
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct BenchmarkResult {
    run_id: String,
    scale: String,
    domain: String,
    relation_mix: String,
    noise: String,
    seed_idx: usize,
    // Bundle structure
    total_nodes: u32,
    total_relations: u32,
    bundle_nodes: u32,
    bundle_relationships: u32,
    // Quality metrics (from kernel domain: rendered.quality)
    rendered_token_count: u32,
    raw_equivalent_tokens: u32,
    compression_ratio: f64,
    causal_density: f64,
    noise_ratio: f64,
    detail_coverage: f64,
    // Kernel domain: resolved mode + tier breakdown
    resolved_mode: String,
    tier_l0_tokens: u32,
    tier_l1_tokens: u32,
    tier_l2_tokens: u32,
    /// Sum of tier tokens. May differ from rendered_token_count because flat
    /// rendering and tiered rendering are independent pipelines over the same bundle.
    tier_total_tokens: u32,
    // Kernel domain: query timing breakdown
    graph_load_ms: f64,
    detail_load_ms: f64,
    bundle_assembly_ms: f64,
    timing_batch_size: u32,
    // Kernel domain: truncation (when budget applied)
    truncation_budget: Option<u32>,
    truncation_used: Option<u32>,
    truncation_sections_dropped: Option<u32>,
    // Kernel domain: served_at
    served_at: Option<String>,
    // Test-side latency
    query_latency_ms: f64,
    total_latency_ms: f64,
    // LLM evaluation verdicts
    llm_task_success: Option<bool>,
    llm_restart_accuracy: Option<bool>,
    llm_restart_exact: Option<bool>,
    llm_restart_off_by_one: Option<bool>,
    llm_restart_on_competing: Option<bool>,
    llm_restart_explained: Option<bool>,
    llm_reason_preserved: Option<bool>,
    llm_reason_correct: Option<bool>,
    llm_reason_distractor: Option<bool>,
    llm_latency_ms: Option<f64>,
    llm_prompt_tokens: Option<u32>,
    llm_completion_tokens: Option<u32>,
    llm_reason_source: Option<String>,
    llm_confidence: Option<String>,
    llm_reason_fabricated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    llm_response: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    judge_raw: Option<String>,
    ground_truth_summary: Option<String>,
}

/// Extract the best available content for LLM evaluation.
fn tier_content_for_eval(rendered: &RenderedContext) -> String {
    if rendered.tiers.is_empty() {
        return rendered.content.clone();
    }
    let mut parts = Vec::new();
    for tier in &rendered.tiers {
        if tier.tier == ResolutionTier::L0Summary as i32
            || tier.tier == ResolutionTier::L1CausalSpine as i32
        {
            parts.push(tier.content.as_str());
        }
    }
    if parts.is_empty() {
        rendered.content.clone()
    } else {
        parts.join("\n\n")
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn env_filter(key: &str) -> Vec<String> {
    std::env::var(key)
        .map(|s| s.split(',').map(str::trim).map(String::from).collect())
        .unwrap_or_default()
}

fn parse_provider(s: &str) -> LlmProvider {
    match s {
        "openai" => LlmProvider::OpenAI,
        "openai-new" => LlmProvider::OpenAINew,
        "anthropic" => LlmProvider::Anthropic,
        other => panic!("unknown provider '{other}' in YAML"),
    }
}

fn yaml_str(cfg: &serde_yaml::Value, field: &str, context: &str) -> String {
    cfg[field]
        .as_str()
        .unwrap_or_else(|| panic!("{context}: missing '{field}' in YAML"))
        .to_string()
}

fn build_llm_config(
    matrix: &serde_yaml::Value,
    agent_cfg: &serde_yaml::Value,
    judge_cfg: &serde_yaml::Value,
) -> LlmEvaluatorConfig {
    let tls = agent_cfg["tls"].as_bool().unwrap_or(false);
    let tls_section = &matrix["tls"];
    let tls_cert = if tls {
        Some(yaml_str(tls_section, "cert", "tls"))
    } else {
        None
    };
    let tls_key = if tls {
        Some(yaml_str(tls_section, "key", "tls"))
    } else {
        None
    };

    LlmEvaluatorConfig {
        endpoint: yaml_str(agent_cfg, "endpoint", "agent"),
        model: yaml_str(agent_cfg, "model", "agent"),
        provider: parse_provider(&yaml_str(agent_cfg, "provider", "agent")),
        api_key: agent_cfg["api_key_env"]
            .as_str()
            .and_then(|e| std::env::var(e).ok()),
        max_tokens: 200,
        temperature: 0.0,
        tls_cert_path: tls_cert,
        tls_key_path: tls_key,
        tls_insecure: tls,
        judge_endpoint: Some(yaml_str(judge_cfg, "endpoint", "judge")),
        judge_model: Some(yaml_str(judge_cfg, "model", "judge")),
        judge_provider: Some(parse_provider(&yaml_str(judge_cfg, "provider", "judge"))),
        judge_api_key: judge_cfg["api_key_env"]
            .as_str()
            .and_then(|e| std::env::var(e).ok()),
        semantic_classifier_endpoint: None,
        semantic_classifier_model: None,
        semantic_classifier_provider: None,
        semantic_classifier_api_key: None,
        semantic_classifier_mode: LlmSemanticClassifierMode::ChatCompletions,
    }
}

// ---------------------------------------------------------------------------
// Extracted helpers to reduce cognitive complexity
// ---------------------------------------------------------------------------

/// Extract quality metrics from rendered context.
struct QualityMetrics {
    raw_equivalent_tokens: u32,
    compression_ratio: f64,
    causal_density: f64,
    noise_ratio: f64,
    detail_coverage: f64,
}

fn extract_quality_metrics(rendered: &RenderedContext) -> QualityMetrics {
    let quality = rendered.quality.as_ref();
    QualityMetrics {
        raw_equivalent_tokens: quality.map(|q| q.raw_equivalent_tokens).unwrap_or(0),
        compression_ratio: quality.map(|q| q.compression_ratio).unwrap_or(0.0),
        causal_density: quality.map(|q| q.causal_density).unwrap_or(0.0),
        noise_ratio: quality.map(|q| q.noise_ratio).unwrap_or(0.0),
        detail_coverage: quality.map(|q| q.detail_coverage).unwrap_or(0.0),
    }
}

/// Extract resolved mode string from rendered context.
fn extract_resolved_mode(rendered: &RenderedContext) -> String {
    match RehydrationMode::try_from(rendered.resolved_mode) {
        Ok(RehydrationMode::ResumeFocused) => "resume_focused",
        Ok(RehydrationMode::ReasonPreserving) => "reason_preserving",
        Ok(RehydrationMode::TemporalDelta) => "temporal_delta",
        Ok(RehydrationMode::GlobalSummary) => "global_summary",
        _ => "auto",
    }
    .to_string()
}

/// Extract per-tier token counts from rendered context.
fn extract_tier_tokens(rendered: &RenderedContext) -> (u32, u32, u32) {
    let l0 = rendered
        .tiers
        .iter()
        .find(|t| t.tier == ResolutionTier::L0Summary as i32)
        .map(|t| t.token_count)
        .unwrap_or(0);
    let l1 = rendered
        .tiers
        .iter()
        .find(|t| t.tier == ResolutionTier::L1CausalSpine as i32)
        .map(|t| t.token_count)
        .unwrap_or(0);
    let l2 = rendered
        .tiers
        .iter()
        .find(|t| t.tier == ResolutionTier::L2EvidencePack as i32)
        .map(|t| t.token_count)
        .unwrap_or(0);
    (l0, l1, l2)
}

/// Build ground truth from a generated seed for LLM evaluation.
fn build_benchmark_ground_truth(
    seed: &GeneratedSeed,
    domain: Domain,
) -> (String, EvaluationGroundTruth) {
    let domain_name_full = match domain {
        Domain::Operations => "operations incident management",
        Domain::SoftwareDebugging => "software debugging",
    };
    let question = format!(
        "Given this rehydrated context from a {domain_name_full} graph:\n\
         1. What is the root cause or failure point?\n\
         2. Which node should the system restart from to continue work?\n\
         3. What is the main rationale in the causal chain?"
    );

    let chain_rationales: Vec<String> = seed
        .relations
        .iter()
        .filter(|r| !r.target_node_id.contains("noise"))
        .filter_map(|r| r.rationale.clone())
        .collect();
    let distractor_rationales: Vec<String> = seed
        .relations
        .iter()
        .filter(|r| r.target_node_id.contains("noise"))
        .filter_map(|r| r.rationale.clone())
        .collect();
    let last_chain_node = seed
        .nodes
        .iter()
        .filter(|n| n.node_kind != "distractor")
        .next_back();

    let ground_truth = EvaluationGroundTruth {
        expected_failure_point: Some(format!(
            "{} ({}) \u{2014} {}",
            last_chain_node.map(|n| n.title.as_str()).unwrap_or("?"),
            last_chain_node.map(|n| n.node_id.as_str()).unwrap_or("?"),
            last_chain_node.map(|n| n.summary.as_str()).unwrap_or("?"),
        )),
        expected_restart_node: Some(format!(
            "{} ({}) \u{2014} causal predecessor",
            seed.nodes
                .iter()
                .filter(|n| n.node_kind != "distractor")
                .rev()
                .nth(1)
                .map(|n| n.title.as_str())
                .unwrap_or("?"),
            seed.nodes
                .iter()
                .filter(|n| n.node_kind != "distractor")
                .rev()
                .nth(1)
                .map(|n| n.node_id.as_str())
                .unwrap_or("?"),
        )),
        expected_reason: if chain_rationales.is_empty() {
            None
        } else {
            Some(chain_rationales.join("; "))
        },
        distractor_rationale: if distractor_rationales.is_empty() {
            None
        } else {
            Some(distractor_rationales.join("; "))
        },
        domain_context: Some(domain_name_full.to_string()),
    };

    (question, ground_truth)
}

/// Build the ground_truth_summary string for the benchmark result.
fn build_ground_truth_summary(seed: &GeneratedSeed) -> String {
    let chain_rationales: Vec<String> = seed
        .relations
        .iter()
        .filter(|r| !r.target_node_id.contains("noise"))
        .filter_map(|r| r.rationale.clone())
        .collect();
    format!(
        "failure={} | restart={} | reason={}",
        seed.nodes
            .iter()
            .filter(|n| n.node_kind != "distractor")
            .next_back()
            .map(|n| format!("{} ({})", n.title, n.node_id))
            .unwrap_or_else(|| "?".to_string()),
        seed.nodes
            .iter()
            .filter(|n| n.node_kind != "distractor")
            .rev()
            .nth(1)
            .map(|n| format!("{} ({})", n.title, n.node_id))
            .unwrap_or_else(|| "?".to_string()),
        chain_rationales.first().unwrap_or(&"none".to_string()),
    )
}

/// Format an optional bool verdict for the summary table.
fn format_bool_verdict(v: Option<bool>) -> &'static str {
    match v {
        Some(true) => "yes",
        Some(false) => "no",
        None => "n/a",
    }
}

/// Print the summary table for benchmark results.
fn print_summary_table(run: &mut RunDir, results: &[BenchmarkResult]) {
    run.log(&format!(
        "{:<40} {:>6} {:>6} {:>6} {:>6} {:>8} {:>8} {:>8}",
        "Variant", "Tok", "Raw", "Compr", "Caus%", "TaskOK", "RestOK", "ReasOK"
    ));
    run.log(&"-".repeat(100));

    for r in results {
        let task = format_bool_verdict(r.llm_task_success);
        let restart = format_bool_verdict(r.llm_restart_accuracy);
        let reason = format_bool_verdict(r.llm_reason_correct);
        run.log(&format!(
            "{:<40} {:>6} {:>6} {:>6.2} {:>5.0}% {:>8} {:>8} {:>8}",
            r.run_id,
            r.rendered_token_count,
            r.raw_equivalent_tokens,
            r.compression_ratio,
            r.causal_density * 100.0,
            task,
            restart,
            reason,
        ));
    }
}

/// Print explanatory vs structural comparison.
fn print_mix_comparison(
    run: &mut RunDir,
    results: &[BenchmarkResult],
    active_scale_names: &[&str],
) {
    run.log("\n--- Explanatory vs Structural ---");
    for scale in active_scale_names {
        for domain in &["ops", "debug"] {
            let exp: Vec<&BenchmarkResult> = results
                .iter()
                .filter(|r| {
                    r.scale == *scale && r.domain == *domain && r.relation_mix == "explanatory"
                })
                .collect();
            let str_: Vec<&BenchmarkResult> = results
                .iter()
                .filter(|r| {
                    r.scale == *scale && r.domain == *domain && r.relation_mix == "structural"
                })
                .collect();

            let exp_task = exp
                .iter()
                .filter(|r| r.llm_task_success == Some(true))
                .count();
            let str_task = str_
                .iter()
                .filter(|r| r.llm_task_success == Some(true))
                .count();
            let exp_n = exp.iter().filter(|r| r.llm_task_success.is_some()).count();
            let str_n = str_.iter().filter(|r| r.llm_task_success.is_some()).count();

            if exp_n > 0 || str_n > 0 {
                run.log(&format!(
                    "  {scale}/{domain}: explanatory={exp_task}/{exp_n} structural={str_task}/{str_n}",
                ));
            }
        }
    }
}

/// Populate LLM evaluation fields on a BenchmarkResult from an eval response.
fn apply_llm_eval(result: &mut BenchmarkResult, eval: &LlmEvaluationResult) {
    result.llm_task_success = Some(eval.llm_task_success);
    result.llm_restart_accuracy = Some(eval.llm_restart_accuracy);
    result.llm_restart_exact = Some(eval.llm_restart_exact);
    result.llm_restart_off_by_one = Some(eval.llm_restart_off_by_one);
    result.llm_restart_on_competing = Some(eval.llm_restart_on_competing);
    result.llm_restart_explained = Some(eval.llm_restart_explained);
    result.llm_reason_preserved = Some(eval.llm_reason_preserved);
    result.llm_reason_correct = Some(eval.llm_reason_correct);
    result.llm_reason_distractor = Some(eval.llm_reason_distractor);
    result.llm_latency_ms = Some(eval.llm_latency_ms);
    result.llm_prompt_tokens = Some(eval.llm_prompt_tokens);
    result.llm_completion_tokens = Some(eval.llm_completion_tokens);
    result.llm_reason_source = Some(eval.llm_reason_source.clone());
    result.llm_confidence = Some(eval.llm_confidence.clone());
    result.llm_reason_fabricated = Some(eval.llm_reason_fabricated);
    result.llm_response = Some(eval.llm_response.clone());
    result.judge_raw = eval.llm_judge_raw.clone();
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

async fn run_vllm_benchmark() -> Result<(), Box<dyn Error + Send + Sync>> {
    let resources = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/kmp-testkit/resources"
    );
    let matrix_path = std::env::var("EVAL_MATRIX_PATH")
        .unwrap_or_else(|_| format!("{resources}/evaluation-matrix.yaml"));

    // ── Load matrix ──
    let matrix: serde_yaml::Value = serde_yaml::from_str(&fs::read_to_string(&matrix_path)?)?;

    // ── Pick first agent + first judge from YAML (or filtered) ──
    let filter_models = env_filter("FILTER_MODELS");
    let filter_judges = env_filter("FILTER_JUDGES");
    let filter_scales = env_filter("FILTER_SCALES");
    let filter_noise = env_filter("FILTER_NOISE");

    let agents = matrix["agents"].as_mapping().expect("agents mapping");
    let judges = matrix["judges"].as_mapping().expect("judges mapping");

    let (agent_name, agent_cfg) = agents
        .iter()
        .find(|(k, _)| {
            let name = k.as_str().unwrap_or("");
            filter_models.is_empty() || filter_models.contains(&name.to_string())
        })
        .map(|(k, v)| (k.as_str().expect("agent key").to_string(), v))
        .expect("no agent matches FILTER_MODELS");

    let (judge_name, judge_cfg) = judges
        .iter()
        .find(|(k, _)| {
            let name = k.as_str().unwrap_or("");
            (filter_judges.is_empty() || filter_judges.contains(&name.to_string()))
                && name != agent_name
        })
        .map(|(k, v)| (k.as_str().expect("judge key").to_string(), v))
        .expect("no judge matches FILTER_JUDGES (excluding self-eval)");

    let llm_config = build_llm_config(&matrix, agent_cfg, judge_cfg);

    // ── Precheck ──
    eprintln!("[PRECHECK] agent={agent_name}, judge={judge_name}");
    eprintln!("[PRECHECK] matrix={matrix_path}");

    // ── RunDir ──
    let mut run = RunDir::create()?;
    run.log(&format!("[CONFIG] matrix={matrix_path}"));
    run.log(&format!("[CONFIG] agent={agent_name}, judge={judge_name}"));

    // ── Judge calibration ──
    let default_prompt = kmp_testkit::PromptConfig::load(None)?;
    run.log(&format!("[CALIBRATION] Testing judge '{judge_name}'..."));
    let cases = calibrate_judge(&default_prompt, &llm_config).await?;
    let mut cal_failed = false;
    for case in &cases {
        let icon = if case.passed { "✔" } else { "✘" };
        run.log(&format!(
            "  {icon} {}: expected={}, got={}",
            case.name, case.expected, case.got
        ));
        if !case.passed {
            cal_failed = true;
        }
    }
    if cal_failed {
        panic!("judge calibration failed for '{judge_name}'");
    }
    run.log(&format!(
        "[CALIBRATION] Judge '{judge_name}' passed ({} cases)\n",
        cases.len()
    ));

    // ── Build variant space from YAML ──
    type ScaleEntry = (&'static str, fn(Domain) -> GraphSeedConfig);
    let all_scales: Vec<ScaleEntry> = vec![
        ("micro", |d| GraphSeedConfig::micro(d)),
        ("meso", |d| GraphSeedConfig::meso(d)),
        ("stress", |d| GraphSeedConfig::stress(d)),
    ];
    let yaml_scales: Vec<String> = matrix["scales"]
        .as_mapping()
        .map(|m| {
            m.keys()
                .filter_map(|k| k.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let scales: Vec<ScaleEntry> = if yaml_scales.is_empty() {
        all_scales
    } else {
        all_scales
            .into_iter()
            .filter(|(name, _)| yaml_scales.iter().any(|s| s == name))
            .collect()
    };

    let domains = [
        ("ops", Domain::Operations),
        ("debug", Domain::SoftwareDebugging),
    ];
    let mixes = [
        ("explanatory", RelationMix::Explanatory),
        ("structural", RelationMix::Structural),
        ("mixed", RelationMix::Mixed),
    ];
    let noise_for_mix: &[(&str, &[(&str, NoiseMode)])] = &[
        (
            "explanatory",
            &[
                ("clean", NoiseMode::Structural),
                ("competing", NoiseMode::CompetingCausal),
            ],
        ),
        (
            "structural",
            &[
                ("clean", NoiseMode::Structural),
                ("conflicting", NoiseMode::ConflictingMainPath),
            ],
        ),
        (
            "mixed",
            &[
                ("clean", NoiseMode::Structural),
                ("restart", NoiseMode::CompetingRestartPoint),
            ],
        ),
    ];

    let seeds_per_cell = matrix["seeds_per_cell"].as_u64().unwrap_or(1) as usize;
    let active_scale_names: Vec<&str> = scales.iter().map(|(name, _)| *name).collect();
    run.log(&format!(
        "[CONFIG] seeds_per_cell={seeds_per_cell}, scales={active_scale_names:?}"
    ));

    let boot_start = Instant::now();
    let mut results: Vec<BenchmarkResult> = Vec::new();

    for &(scale_name, scale_fn) in &scales {
        if !filter_scales.is_empty() && !filter_scales.contains(&scale_name.to_string()) {
            continue;
        }
        for &(domain_name, domain) in &domains {
            for &(mix_name, mix) in &mixes {
                let noises = noise_for_mix
                    .iter()
                    .find(|(m, _)| *m == mix_name)
                    .map(|(_, n)| *n)
                    .unwrap_or(&[("clean", NoiseMode::Structural)]);

                for &(noise_name, noise_mode) in noises {
                    if !filter_noise.is_empty() && !filter_noise.contains(&noise_name.to_string()) {
                        continue;
                    }
                    for seed_idx in 0..seeds_per_cell {
                        let variant_id = if seeds_per_cell > 1 {
                            format!(
                                "{scale_name}-{domain_name}-{mix_name}-{noise_name}-s{seed_idx}"
                            )
                        } else {
                            format!("{scale_name}-{domain_name}-{mix_name}-{noise_name}")
                        };

                        let mut config = scale_fn(domain);
                        config.relation_mix = mix;
                        config.noise_mode = noise_mode;
                        config.id_prefix = variant_id.clone();
                        config.seed = seed_idx;
                        let seed = generate_seed(config.clone());

                        let events = seed_to_projection_events(&seed, SUBJECT_PREFIX, &variant_id)?;
                        let root_id = seed.root.node_id.clone();
                        let focus_id = seed
                            .nodes
                            .first()
                            .map(|n| n.node_id.clone())
                            .unwrap_or_else(|| "chain-0".to_string());

                        let fixture = TestFixture::builder()
                            .with_neo4j()
                            .with_valkey()
                            .with_nats()
                            .with_projection_runtime()
                            .with_grpc_server()
                            .with_seed(ClosureSeed::new(move |ctx: &SeedContext| {
                                let events = events.clone();
                                Box::pin(async move {
                                    let publisher = ctx.nats_client();
                                    for (subject, payload) in events {
                                        publisher.publish(subject, payload.into()).await?;
                                    }
                                    publisher.flush().await?;
                                    Ok(())
                                })
                            }))
                            .with_readiness_check(&root_id, &focus_id)
                            .build()
                            .await?;

                        sleep(Duration::from_secs(2)).await;

                        let result: Result<BenchmarkResult, Box<dyn Error + Send + Sync>> = async {
                            let mut query_client = fixture.query_client();
                            let total_start = Instant::now();
                            let query_start = Instant::now();

                            let response = query_client
                                .get_context(GetContextRequest {
                                    root_node_id: seed.root.node_id.clone(),
                                    role: BENCHMARK_ROLE.to_string(),
                                    token_budget: std::env::var("BENCHMARK_TOKEN_BUDGET")
                                        .ok()
                                        .and_then(|v| v.parse().ok())
                                        .unwrap_or(4096),
                                    requested_scopes: vec![],
                                    depth: config.chain_length as u32,
                                    max_tier: 0,
                                    rehydration_mode: 0,
                                })
                                .await?
                                .into_inner();

                            let query_latency_ms = query_start.elapsed().as_secs_f64() * 1000.0;
                            let bundle = response.bundle.ok_or("missing bundle")?;
                            let role_bundle = bundle.bundles.first().ok_or("missing role bundle")?;
                            let rendered = response.rendered.ok_or("missing rendered context")?;

                            // Extract metrics using helpers
                            let qm = extract_quality_metrics(&rendered);
                            let resolved_mode = extract_resolved_mode(&rendered);
                            let (tier_l0_tokens, tier_l1_tokens, tier_l2_tokens) =
                                extract_tier_tokens(&rendered);

                            // Truncation from kernel domain (rendered.truncation)
                            let truncation = rendered.truncation.as_ref();
                            let truncation_budget = truncation.map(|t| t.budget_requested);
                            let truncation_used = truncation.map(|t| t.budget_used);
                            let truncation_sections_dropped = truncation.map(|t| t.sections_dropped);

                            // served_at from kernel domain
                            let served_at = response.served_at.as_ref().map(|t| {
                                format!("{}s", t.seconds)
                            });

                            // Query timing breakdown from kernel domain (response.timing)
                            let timing = response.timing.as_ref();
                            let graph_load_ms = timing.map(|t| t.graph_load_seconds * 1000.0).unwrap_or(0.0);
                            let detail_load_ms = timing.map(|t| t.detail_load_seconds * 1000.0).unwrap_or(0.0);
                            let bundle_assembly_ms = timing.map(|t| t.bundle_assembly_seconds * 1000.0).unwrap_or(0.0);
                            let timing_batch_size = timing.map(|t| t.batch_size).unwrap_or(0);

                            run.log(&format!(
                                "[CAPTURE] {variant_id}: {} tok (raw={}, compress={:.2}x, causal={:.0}%, noise={:.0}%, detail={:.0}%) mode={resolved_mode} L0={tier_l0_tokens} L1={tier_l1_tokens} L2={tier_l2_tokens} graph={graph_load_ms:.0}ms detail={detail_load_ms:.0}ms assembly={bundle_assembly_ms:.0}ms",
                                rendered.token_count, qm.raw_equivalent_tokens, qm.compression_ratio,
                                qm.causal_density * 100.0, qm.noise_ratio * 100.0, qm.detail_coverage * 100.0,
                            ));

                            // LLM evaluation
                            let (question, ground_truth) =
                                build_benchmark_ground_truth(&seed, domain);

                            let eval_content = tier_content_for_eval(&rendered);
                            let llm_eval = match evaluate_with_llm(&llm_config, &eval_content, &question, &ground_truth).await {
                                Ok(eval) => Some(eval),
                                Err(error) => {
                                    run.log(&format!("[EVAL] {variant_id}: LLM eval failed: {error}"));
                                    None
                                }
                            };

                            let total_latency_ms = total_start.elapsed().as_secs_f64() * 1000.0;

                            if let Some(ref eval) = llm_eval {
                                let task = if eval.llm_task_success { "OK" } else { "FAIL" };
                                let restart = if eval.llm_restart_accuracy { "OK" } else { "FAIL" };
                                let reason = if eval.llm_reason_correct { "OK" } else { "FAIL" };
                                run.log(&format!("[EVAL] {variant_id}: Task={task} Restart={restart} Reason={reason} ({:.0}ms)", eval.llm_latency_ms));
                            }

                            let mut result = BenchmarkResult {
                                run_id: variant_id.clone(),
                                scale: scale_name.to_string(),
                                domain: domain_name.to_string(),
                                relation_mix: mix_name.to_string(),
                                noise: noise_name.to_string(),
                                seed_idx,
                                total_nodes: seed.total_nodes() as u32,
                                total_relations: seed.total_relations() as u32,
                                bundle_nodes: role_bundle.neighbor_nodes.len() as u32 + 1,
                                bundle_relationships: role_bundle.relationships.len() as u32,
                                rendered_token_count: rendered.token_count,
                                raw_equivalent_tokens: qm.raw_equivalent_tokens,
                                compression_ratio: qm.compression_ratio,
                                causal_density: qm.causal_density,
                                noise_ratio: qm.noise_ratio,
                                detail_coverage: qm.detail_coverage,
                                resolved_mode,
                                tier_l0_tokens,
                                tier_l1_tokens,
                                tier_l2_tokens,
                                tier_total_tokens: tier_l0_tokens + tier_l1_tokens + tier_l2_tokens,
                                graph_load_ms,
                                detail_load_ms,
                                bundle_assembly_ms,
                                timing_batch_size,
                                truncation_budget,
                                truncation_used,
                                truncation_sections_dropped,
                                served_at,
                                query_latency_ms,
                                total_latency_ms,
                                llm_task_success: None,
                                llm_restart_accuracy: None,
                                llm_restart_exact: None,
                                llm_restart_off_by_one: None,
                                llm_restart_on_competing: None,
                                llm_restart_explained: None,
                                llm_reason_preserved: None,
                                llm_reason_correct: None,
                                llm_reason_distractor: None,
                                llm_latency_ms: None,
                                llm_prompt_tokens: None,
                                llm_completion_tokens: None,
                                llm_reason_source: None,
                                llm_confidence: None,
                                llm_reason_fabricated: None,
                                llm_response: None,
                                judge_raw: None,
                                ground_truth_summary: Some(build_ground_truth_summary(&seed)),
                            };

                            if let Some(ref eval) = llm_eval {
                                apply_llm_eval(&mut result, eval);
                            }

                            Ok(result)
                        }
                        .await;

                        fixture.shutdown().await?;
                        let result = result?;
                        run.write_result(&variant_id, &result)?;
                        results.push(result);
                    }
                }
            }
        }
    }

    let total_ms = boot_start.elapsed().as_secs_f64() * 1000.0;

    // ── Summary table ──
    run.log(&format!(
        "\n=== vLLM Kernel Benchmark ({} evals, {total_ms:.0}ms) ===\n",
        results.len()
    ));
    print_summary_table(&mut run, &results);

    // ── Explanatory vs structural comparison ──
    print_mix_comparison(&mut run, &results, &active_scale_names);

    // ── Write summary JSON ──
    let summary = serde_json::to_vec_pretty(&results)?;
    fs::write(run.path.join("summary.json"), summary)?;
    run.log(&format!(
        "\n[RUN] results written to {}",
        run.path.display()
    ));

    // ── Structural invariants (always, even without LLM) ──
    for r in &results {
        assert!(r.bundle_nodes > 0, "{}: bundle should have nodes", r.run_id);
        assert!(
            r.rendered_token_count > 0,
            "{}: should have tokens",
            r.run_id
        );
    }

    Ok(())
}

#[tokio::test]
async fn vllm_benchmark_across_scales_domains_and_variants()
-> Result<(), Box<dyn Error + Send + Sync>> {
    run_vllm_benchmark().await
}
