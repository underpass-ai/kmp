#![cfg(feature = "container-tests")]
use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use kmp_proto::v1beta1::{GetContextRequest, RehydrationMode, RenderedContext, ResolutionTier};
use kmp_testkit::{
    Domain, EvaluationGroundTruth, GeneratedNode, GeneratedSeed, GraphSeedConfig,
    LlmEvaluationResult, LlmEvaluatorConfig, LlmProvider, LlmSemanticClassifierMode, NoiseMode,
    PromptConfig, RelationMix, calibrate_agent, calibrate_judge, evaluate_with_config,
    generate_seed, seed_publisher::seed_to_projection_events,
};
use kmp_tests_shared::containers::connect_nats_with_retry;
use kmp_tests_shared::fixtures::TestFixture;
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
        let path = base.join(ts);
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
        result: &EvalResult,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let json = serde_json::to_vec_pretty(result)?;
        fs::write(self.path.join("results").join(format!("{name}.json")), json)?;
        Ok(())
    }

    fn write_summary(
        &mut self,
        results: &[EvalResult],
        captured: &[CapturedVariant],
        boot_ms: f64,
        total_ms: f64,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // JSON summary
        let summary = serde_json::to_vec_pretty(results)?;
        fs::write(self.path.join("summary.json"), summary)?;

        // Markdown report
        let mut md = String::new();
        md.push_str("# E2E Evaluation Matrix\n\n");
        md.push_str(&format!("- **Variants captured**: {}\n", captured.len()));
        md.push_str(&format!("- **Evaluations**: {}\n", results.len()));
        md.push_str(&format!("- **Boot time**: {boot_ms:.0}ms\n"));
        md.push_str(&format!("- **Total time**: {total_ms:.0}ms\n\n"));

        md.push_str("| Model | Prompt | Variant | Task | Restart | Reason | Latency |\n");
        md.push_str("|-------|--------|---------|------|---------|--------|---------|\n");
        for r in results {
            let t = match r.task {
                Some(true) => "OK",
                Some(false) => "FAIL",
                None => "ERR",
            };
            let re = match r.restart {
                Some(true) => "OK",
                Some(false) => "FAIL",
                None => "ERR",
            };
            let p = match r.reason {
                Some(true) => "OK",
                Some(false) => "FAIL",
                None => "ERR",
            };
            md.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {:.0}ms |\n",
                r.model, r.prompt, r.variant, t, re, p, r.latency_ms
            ));
        }

        // Aggregates by model x prompt
        md.push_str("\n## Aggregates\n\n");
        md.push_str("| Model | Prompt | Task | Restart | Reason |\n");
        md.push_str("|-------|--------|------|---------|--------|\n");
        let mut seen: Vec<(String, String)> = Vec::new();
        for r in results {
            let key = (r.model.clone(), r.prompt.clone());
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            let cell: Vec<&EvalResult> = results
                .iter()
                .filter(|x| x.model == r.model && x.prompt == r.prompt)
                .collect();
            let n = cell.iter().filter(|x| x.task.is_some()).count();
            let t = cell.iter().filter(|x| x.task == Some(true)).count();
            let re = cell.iter().filter(|x| x.restart == Some(true)).count();
            let p = cell.iter().filter(|x| x.reason == Some(true)).count();
            md.push_str(&format!(
                "| {} | {} | {}/{} | {}/{} | {}/{} |\n",
                r.model, r.prompt, t, n, re, n, p, n
            ));
        }

        fs::write(self.path.join("report.md"), md)?;

        // Log the path
        self.log(&format!("[RUN] results written to {}", self.path.display()));
        Ok(())
    }
}

fn chrono_stamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    // Simple UTC timestamp without chrono dependency
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;
    // Approximate date from epoch days (good enough for directory names)
    let (year, month, day) = epoch_days_to_date(days);
    format!("{year:04}-{month:02}-{day:02}_{hours:02}{minutes:02}{seconds:02}")
}

fn epoch_days_to_date(days: u64) -> (u64, u64, u64) {
    // Adapted from Howard Hinnant's algorithm
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
// Domain types
// ---------------------------------------------------------------------------

struct CapturedVariant {
    run_id: String,
    rendered_content: String,
    rendered_tokens: u32,
    raw_equivalent_tokens: u32,
    compression_ratio: f64,
    causal_density: f64,
    noise_ratio: f64,
    detail_coverage: f64,
    // Kernel domain observability
    resolved_mode: String,
    tier_l0_tokens: u32,
    tier_l1_tokens: u32,
    tier_l2_tokens: u32,
    tier_total_tokens: u32,
    graph_load_ms: f64,
    detail_load_ms: f64,
    bundle_assembly_ms: f64,
    timing_batch_size: u32,
    question: String,
    ground_truth: EvaluationGroundTruth,
}

#[derive(Serialize)]
struct EvalResult {
    model: String,
    prompt: String,
    variant: String,
    task: Option<bool>,
    restart: Option<bool>,
    restart_exact: Option<bool>,
    restart_off_by_one: Option<bool>,
    restart_on_competing: Option<bool>,
    restart_explained: Option<bool>,
    reason: Option<bool>,
    reason_correct: Option<bool>,
    reason_distractor: Option<bool>,
    latency_ms: f64,
    rendered_tokens: u32,
    raw_equivalent_tokens: u32,
    compression_ratio: f64,
    causal_density: f64,
    noise_ratio: f64,
    detail_coverage: f64,
    // Kernel domain observability
    resolved_mode: String,
    tier_l0_tokens: u32,
    tier_l1_tokens: u32,
    tier_l2_tokens: u32,
    tier_total_tokens: u32,
    graph_load_ms: f64,
    detail_load_ms: f64,
    bundle_assembly_ms: f64,
    timing_batch_size: u32,
    llm_prompt_tokens: Option<u32>,
    llm_completion_tokens: Option<u32>,
    llm_reason_source: Option<String>,
    llm_confidence: Option<String>,
    llm_reason_fabricated: Option<bool>,
    agent_response: String,
    judge_raw: Option<String>,
}

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

fn check_api_endpoint(
    endpoint: &str,
    api_key: Option<&str>,
    tls_cert: Option<&str>,
    tls_key: Option<&str>,
) -> bool {
    let probe_url = endpoint
        .replace("/v1/chat/completions", "/v1/models")
        .replace("/v1/messages", "/v1/models");

    let mut args = vec![
        "-s",
        "--max-time",
        "10",
        "-o",
        "/dev/null",
        "-w",
        "%{http_code}",
    ];
    if tls_cert.is_some() {
        args.push("-k");
    }
    if let Some(cert) = tls_cert {
        args.push("--cert");
        args.push(cert);
    }
    if let Some(key) = tls_key {
        args.push("--key");
        args.push(key);
    }

    let auth_header;
    let anthropic_header;
    let anthropic_version;
    if let Some(key) = api_key {
        if endpoint.contains("anthropic.com") {
            anthropic_header = format!("x-api-key: {key}");
            anthropic_version = "anthropic-version: 2023-06-01".to_string();
            args.extend(["-H", &anthropic_header, "-H", &anthropic_version]);
        } else {
            auth_header = format!("Authorization: Bearer {key}");
            args.extend(["-H", &auth_header]);
        }
    }
    args.push(&probe_url);

    match std::process::Command::new("curl").args(&args).output() {
        Ok(out) => {
            let code = String::from_utf8_lossy(&out.stdout);
            code.trim() != "000"
        }
        Err(_) => false,
    }
}

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
        other => {
            panic!("unknown provider '{other}' in YAML — expected: openai, openai-new, anthropic")
        }
    }
}

fn yaml_str(cfg: &serde_yaml::Value, field: &str, context: &str) -> String {
    cfg[field]
        .as_str()
        .unwrap_or_else(|| panic!("{context}: missing required field '{field}' in YAML"))
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

async fn wait_for_context(
    fixture: &TestFixture,
    root_node_id: &str,
    _focus_node_id: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut qc = fixture.query_client();
    for _ in 0..40 {
        if let Ok(resp) = qc
            .get_context(GetContextRequest {
                root_node_id: root_node_id.to_string(),
                role: BENCHMARK_ROLE.to_string(),
                token_budget: 1200,
                requested_scopes: vec!["implementation".to_string()],
                depth: 0,
                max_tier: 0,
                rehydration_mode: 0,
            })
            .await
        {
            let resp = resp.into_inner();
            if let Some(bundle) = resp.bundle
                && bundle.root_node_id == root_node_id
                && bundle
                    .bundles
                    .first()
                    .is_some_and(|b| !b.neighbor_nodes.is_empty())
            {
                return Ok(());
            }
        }
        sleep(Duration::from_millis(200)).await;
    }
    Err("context did not become ready".into())
}

// ---------------------------------------------------------------------------
// Precheck — validate all dependencies before booting containers
// ---------------------------------------------------------------------------

struct PrecheckResult {
    pass: bool,
    ok: Vec<String>,
    warnings: Vec<String>,
    errors: Vec<String>,
}

fn precheck_tls(matrix: &serde_yaml::Value, ok: &mut Vec<String>, errors: &mut Vec<String>) {
    let any_tls_agent = matrix["agents"]
        .as_mapping()
        .map(|a| a.values().any(|c| c["tls"].as_bool().unwrap_or(false)))
        .unwrap_or(false);
    if any_tls_agent {
        match (
            matrix["tls"]["cert"].as_str(),
            matrix["tls"]["key"].as_str(),
        ) {
            (Some(c), Some(k)) => {
                ok.push(format!("tls section: cert={c}, key={k}"));
            }
            _ => {
                errors.push(
                    "tls.cert and tls.key required in YAML when any agent has tls: true"
                        .to_string(),
                );
            }
        }
    }
}

fn load_matrix(matrix_path: &str) -> Result<serde_yaml::Value, String> {
    let content = std::fs::read_to_string(matrix_path)
        .map_err(|e| format!("evaluation-matrix.yaml not found: {e}"))?;
    serde_yaml::from_str::<serde_yaml::Value>(&content)
        .map_err(|e| format!("evaluation-matrix.yaml parse error: {e}"))
}

fn validate_matrix(
    matrix: &serde_yaml::Value,
    resources: &str,
    filter_models: &[String],
    filter_judges: &[String],
    ok: &mut Vec<String>,
    errors: &mut Vec<String>,
) {
    let valid_providers = ["openai", "openai-new", "anthropic"];

    precheck_tls(matrix, ok, errors);
    precheck_agents(matrix, filter_models, &valid_providers, ok, errors);
    precheck_judges(matrix, filter_judges, &valid_providers, ok, errors);
    precheck_prompts(matrix, resources, ok, errors);
}

fn filter_allows(name: &str, filter: &[String]) -> bool {
    filter.is_empty() || filter.contains(&name.to_string())
}

fn yaml_endpoint(cfg: &serde_yaml::Value) -> Option<&str> {
    cfg["endpoint"].as_str()
}

fn yaml_model(cfg: &serde_yaml::Value) -> Option<&str> {
    cfg["model"].as_str()
}

fn yaml_provider(cfg: &serde_yaml::Value) -> Option<&str> {
    cfg["provider"].as_str()
}

fn push_missing_field(errors: &mut Vec<String>, kind: &str, name: &str, field: &str) {
    errors.push(format!("{kind} '{name}': missing {field}"));
}

fn push_unknown_provider(
    errors: &mut Vec<String>,
    kind: &str,
    name: &str,
    provider: &str,
    valid_providers: &[&str],
) {
    errors.push(format!(
        "{kind} '{name}': unknown provider '{provider}' — expected: {}",
        valid_providers.join(", ")
    ));
}

fn check_endpoint_reachable(
    endpoint: &str,
    api_key: Option<&str>,
    cert: Option<&str>,
    key: Option<&str>,
) -> bool {
    check_api_endpoint(endpoint, api_key, cert, key)
}

fn precheck_named_service(
    kind: &str,
    name: &str,
    cfg: &serde_yaml::Value,
    valid_providers: &[&str],
    ok: &mut Vec<String>,
    errors: &mut Vec<String>,
) {
    if yaml_endpoint(cfg).unwrap_or("").is_empty() {
        push_missing_field(errors, kind, name, "endpoint");
    }
    if yaml_model(cfg).unwrap_or("").is_empty() {
        push_missing_field(errors, kind, name, "model");
    }
    match yaml_provider(cfg) {
        None => push_missing_field(errors, kind, name, "provider"),
        Some(provider) if !valid_providers.contains(&provider) => {
            push_unknown_provider(errors, kind, name, provider, valid_providers);
        }
        Some(_) => {}
    }

    if let Some(env_name) = cfg["api_key_env"].as_str() {
        if let Ok(api_key) = std::env::var(env_name) {
            ok.push(format!("{kind} '{name}': {env_name} set"));
            if let Some(endpoint) = yaml_endpoint(cfg) {
                if check_endpoint_reachable(endpoint, Some(&api_key), None, None) {
                    ok.push(format!("{kind} '{name}': endpoint reachable"));
                } else {
                    errors.push(format!(
                        "{kind} '{name}': endpoint unreachable ({endpoint})"
                    ));
                }
            }
        } else {
            errors.push(format!("{kind} '{name}': env var {env_name} not set"));
        }
    }
}

fn precheck_agent_tls_paths(
    name: &str,
    matrix: &serde_yaml::Value,
    cfg: &serde_yaml::Value,
    ok: &mut Vec<String>,
    errors: &mut Vec<String>,
) {
    if !cfg["tls"].as_bool().unwrap_or(false) {
        return;
    }
    match (
        matrix["tls"]["cert"].as_str(),
        matrix["tls"]["key"].as_str(),
    ) {
        (None, _) => errors.push(format!("agent '{name}': tls.cert not set in YAML")),
        (_, None) => errors.push(format!("agent '{name}': tls.key not set in YAML")),
        (Some(cert), Some(key)) => {
            if !std::path::Path::new(cert).exists() {
                errors.push(format!(
                    "agent '{name}': TLS cert not found at {cert} (tls.cert in YAML)"
                ));
            } else if !std::path::Path::new(key).exists() {
                errors.push(format!(
                    "agent '{name}': TLS key not found at {key} (tls.key in YAML)"
                ));
            } else {
                ok.push(format!("agent '{name}': TLS certs present ({cert}, {key})"));
                if let Some(endpoint) = yaml_endpoint(cfg) {
                    if check_endpoint_reachable(endpoint, None, Some(cert), Some(key)) {
                        ok.push(format!("agent '{name}': TLS endpoint reachable"));
                    } else {
                        errors.push(format!(
                            "agent '{name}': TLS endpoint unreachable ({endpoint})"
                        ));
                    }
                }
            }
        }
    }
}

fn precheck_agent_entry(
    name: &str,
    cfg: &serde_yaml::Value,
    matrix: &serde_yaml::Value,
    valid_providers: &[&str],
    ok: &mut Vec<String>,
    errors: &mut Vec<String>,
) {
    precheck_named_service("agent", name, cfg, valid_providers, ok, errors);
    precheck_agent_tls_paths(name, matrix, cfg, ok, errors);
}

fn precheck_judge_entry(
    name: &str,
    cfg: &serde_yaml::Value,
    valid_providers: &[&str],
    ok: &mut Vec<String>,
    errors: &mut Vec<String>,
) {
    precheck_named_service("judge", name, cfg, valid_providers, ok, errors);
}

fn precheck_agents(
    matrix: &serde_yaml::Value,
    filter_models: &[String],
    valid_providers: &[&str],
    ok: &mut Vec<String>,
    errors: &mut Vec<String>,
) {
    if let Some(agents) = matrix["agents"].as_mapping() {
        ok.push(format!("{} agents configured", agents.len()));
        for (key, cfg) in agents {
            let name = key.as_str().unwrap_or("?");
            if !filter_allows(name, filter_models) {
                ok.push(format!("agent '{name}': skipped (filtered)"));
                continue;
            }
            precheck_agent_entry(name, cfg, matrix, valid_providers, ok, errors);
        }
    } else {
        errors.push("matrix: no agents defined".to_string());
    }
}

fn precheck_judges(
    matrix: &serde_yaml::Value,
    filter_judges: &[String],
    valid_providers: &[&str],
    ok: &mut Vec<String>,
    errors: &mut Vec<String>,
) {
    if let Some(judges) = matrix["judges"].as_mapping() {
        ok.push(format!("{} judges configured", judges.len()));
        for (key, cfg) in judges {
            let name = key.as_str().unwrap_or("?");
            if !filter_allows(name, filter_judges) {
                ok.push(format!("judge '{name}': skipped (filtered)"));
                continue;
            }
            precheck_judge_entry(name, cfg, valid_providers, ok, errors);
        }
    } else {
        errors.push("matrix: no judges defined".to_string());
    }
}

fn precheck_prompts(
    matrix: &serde_yaml::Value,
    resources: &str,
    ok: &mut Vec<String>,
    errors: &mut Vec<String>,
) {
    if let Some(prompts) = matrix["prompts"].as_mapping() {
        for (key, val) in prompts {
            let name = key.as_str().unwrap_or("?");
            if let Some(path) = val.as_str() {
                let full_path = format!("{resources}/{path}");
                if std::path::Path::new(&full_path).exists() {
                    ok.push(format!("prompt '{name}': {path}"));
                } else {
                    errors.push(format!("prompt '{name}': file not found at {full_path}"));
                }
            }
            // null means compiled-in default — always ok
        }
    }
}

fn precheck_container_runtime(ok: &mut Vec<String>, errors: &mut Vec<String>) {
    let has_docker = std::process::Command::new("docker")
        .arg("info")
        .output()
        .is_ok_and(|o| o.status.success());
    let has_podman = std::process::Command::new("podman")
        .arg("info")
        .output()
        .is_ok_and(|o| o.status.success());
    if has_docker {
        ok.push("container runtime: docker".to_string());
    } else if has_podman {
        ok.push("container runtime: podman".to_string());
    } else {
        errors.push("no container runtime: install docker or podman".to_string());
    }
}

fn precheck_filters(warnings: &mut Vec<String>) {
    for filter in &[
        "FILTER_MODELS",
        "FILTER_PROMPTS",
        "FILTER_SCALES",
        "FILTER_NOISE",
        "FILTER_JUDGES",
    ] {
        if let Ok(val) = std::env::var(filter) {
            warnings.push(format!("{filter}={val} (subset mode)"));
        }
    }
}

fn precheck(resources: &str, matrix_path: &str) -> PrecheckResult {
    let mut ok = Vec::new();
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    let filter_models = env_filter("FILTER_MODELS");
    let filter_judges = env_filter("FILTER_JUDGES");

    // 1. Matrix YAML
    let matrix_path = matrix_path.to_string();
    match load_matrix(&matrix_path) {
        Ok(matrix) => {
            ok.push(format!("evaluation-matrix.yaml loaded ({matrix_path})"));
            validate_matrix(
                &matrix,
                resources,
                &filter_models,
                &filter_judges,
                &mut ok,
                &mut errors,
            );
        }
        Err(msg) => errors.push(msg),
    }

    // 3. Container runtime
    precheck_container_runtime(&mut ok, &mut errors);

    // 4. Filters (informational)
    precheck_filters(&mut warnings);

    PrecheckResult {
        pass: errors.is_empty(),
        ok,
        warnings,
        errors,
    }
}

// ---------------------------------------------------------------------------
// Phase 1 helpers: ground truth + kernel observability extraction
// ---------------------------------------------------------------------------

fn build_ground_truth_for_seed(
    seed: &GeneratedSeed,
    domain_name: &str,
) -> (String, EvaluationGroundTruth) {
    let chain_nodes: Vec<&GeneratedNode> = seed
        .nodes
        .iter()
        .filter(|n| !n.node_id.contains("noise") && !n.node_id.contains("distractor"))
        .collect();

    let question = format!(
        "Given this rehydrated context from a {domain_name} graph:\n\
         1. What is the deepest failure point in the causal chain?\n\
         2. Which node should the system restart from to recover?\n\
         3. What rationale connects the nodes in the causal chain?"
    );

    let failure_desc = if let Some(leaf) = chain_nodes.last() {
        format!(
            "{} ({}) \u{2014} {}. Causal chain: {} \u{2192} {}",
            leaf.title,
            leaf.node_id,
            leaf.summary,
            seed.root.title,
            chain_nodes
                .iter()
                .map(|n| n.title.as_str())
                .collect::<Vec<_>>()
                .join(" \u{2192} ")
        )
    } else {
        format!(
            "{} \u{2014} {} (single node, no chain)",
            seed.root.node_kind, seed.root.summary
        )
    };

    let restart_desc = if chain_nodes.len() >= 2 {
        let predecessor = &chain_nodes[chain_nodes.len() - 2];
        format!(
            "{} ({}) \u{2014} causal predecessor of the failure leaf",
            predecessor.title, predecessor.node_id
        )
    } else if let Some(first) = chain_nodes.first() {
        format!(
            "{} ({}) \u{2014} first node after root",
            first.title, first.node_id
        )
    } else {
        format!("{} (root, only node)", seed.root.node_kind)
    };

    let chain_rationales: Vec<String> = seed
        .relations
        .iter()
        .filter(|r| !r.source_node_id.contains("noise"))
        .filter_map(|r| r.rationale.clone())
        .collect();
    let reason = if chain_rationales.is_empty() {
        None
    } else {
        Some(chain_rationales.join("; "))
    };

    let distractor_rationales: Vec<String> = seed
        .relations
        .iter()
        .filter(|r| r.source_node_id.contains("noise") || r.target_node_id.contains("noise"))
        .filter_map(|r| r.rationale.clone())
        .collect();
    let distractor = if distractor_rationales.is_empty() {
        None
    } else {
        Some(distractor_rationales.join("; "))
    };

    let ground_truth = EvaluationGroundTruth {
        expected_failure_point: Some(failure_desc),
        expected_restart_node: Some(restart_desc),
        expected_reason: reason,
        distractor_rationale: distractor,
        domain_context: Some(domain_name.to_string()),
    };

    (question, ground_truth)
}

struct KernelObservability {
    resolved_mode: String,
    tier_l0_tokens: u32,
    tier_l1_tokens: u32,
    tier_l2_tokens: u32,
    tier_total_tokens: u32,
    graph_load_ms: f64,
    detail_load_ms: f64,
    bundle_assembly_ms: f64,
    timing_batch_size: u32,
}

fn extract_kernel_observability(
    rendered: &RenderedContext,
    graph_load_ms: f64,
    detail_load_ms: f64,
    bundle_assembly_ms: f64,
    timing_batch_size: u32,
) -> KernelObservability {
    let resolved_mode = match RehydrationMode::try_from(rendered.resolved_mode) {
        Ok(RehydrationMode::ResumeFocused) => "resume_focused",
        Ok(RehydrationMode::ReasonPreserving) => "reason_preserving",
        Ok(RehydrationMode::TemporalDelta) => "temporal_delta",
        Ok(RehydrationMode::GlobalSummary) => "global_summary",
        _ => "auto",
    }
    .to_string();
    let tier_l0_tokens = rendered
        .tiers
        .iter()
        .find(|t| t.tier == ResolutionTier::L0Summary as i32)
        .map(|t| t.token_count)
        .unwrap_or(0);
    let tier_l1_tokens = rendered
        .tiers
        .iter()
        .find(|t| t.tier == ResolutionTier::L1CausalSpine as i32)
        .map(|t| t.token_count)
        .unwrap_or(0);
    let tier_l2_tokens = rendered
        .tiers
        .iter()
        .find(|t| t.tier == ResolutionTier::L2EvidencePack as i32)
        .map(|t| t.token_count)
        .unwrap_or(0);

    KernelObservability {
        resolved_mode,
        tier_l0_tokens,
        tier_l1_tokens,
        tier_l2_tokens,
        tier_total_tokens: tier_l0_tokens + tier_l1_tokens + tier_l2_tokens,
        graph_load_ms,
        detail_load_ms,
        bundle_assembly_ms,
        timing_batch_size,
    }
}

// ---------------------------------------------------------------------------
// Phase 2 helpers: evaluation result construction
// ---------------------------------------------------------------------------

fn build_eval_result_from_ctx(
    ctx: &CapturedVariant,
    model: String,
    prompt_name: &str,
) -> EvalResult {
    EvalResult {
        model,
        prompt: prompt_name.to_string(),
        variant: ctx.run_id.clone(),
        task: None,
        restart: None,
        restart_exact: None,
        restart_off_by_one: None,
        restart_on_competing: None,
        restart_explained: None,
        reason: None,
        reason_correct: None,
        reason_distractor: None,
        latency_ms: 0.0,
        rendered_tokens: ctx.rendered_tokens,
        raw_equivalent_tokens: ctx.raw_equivalent_tokens,
        compression_ratio: ctx.compression_ratio,
        causal_density: ctx.causal_density,
        noise_ratio: ctx.noise_ratio,
        detail_coverage: ctx.detail_coverage,
        resolved_mode: ctx.resolved_mode.clone(),
        tier_l0_tokens: ctx.tier_l0_tokens,
        tier_l1_tokens: ctx.tier_l1_tokens,
        tier_l2_tokens: ctx.tier_l2_tokens,
        tier_total_tokens: ctx.tier_total_tokens,
        graph_load_ms: ctx.graph_load_ms,
        detail_load_ms: ctx.detail_load_ms,
        bundle_assembly_ms: ctx.bundle_assembly_ms,
        timing_batch_size: ctx.timing_batch_size,
        llm_prompt_tokens: None,
        llm_completion_tokens: None,
        llm_reason_source: None,
        llm_confidence: None,
        llm_reason_fabricated: None,
        agent_response: String::new(),
        judge_raw: None,
    }
}

// ---------------------------------------------------------------------------
// Phase 3 helpers: report generation
// ---------------------------------------------------------------------------

fn format_verdict(v: Option<bool>) -> &'static str {
    match v {
        Some(true) => "OK",
        Some(false) => "FAIL",
        None => "ERR",
    }
}

fn write_report(
    run: &mut RunDir,
    results: &[EvalResult],
    captured: &[CapturedVariant],
    boot_ms: f64,
    total_ms: f64,
) {
    run.log(&format!("\n\n{}", "=".repeat(120)));
    run.log(&format!(
        "EVALUATION MATRIX \u{2014} {} variants captured, {} evals, boot {boot_ms:.0}ms, total {total_ms:.0}ms",
        captured.len(),
        results.len()
    ));
    run.log(&format!("{}\n", "=".repeat(120)));

    for r in results {
        let ctx = captured.iter().find(|c| c.run_id == r.variant);
        let question = ctx.map(|c| c.question.as_str()).unwrap_or("?");
        let expected = ctx
            .and_then(|c| c.ground_truth.expected_reason.as_deref())
            .unwrap_or("none");
        let tokens = ctx.map(|c| c.rendered_tokens).unwrap_or(0);

        let t = format_verdict(r.task);
        let re = format_verdict(r.restart);
        let p = format_verdict(r.reason);

        run.log(&format!(
            "\u{250c}\u{2500}\u{2500} {:<14} \u{00d7} {:<14} / {} ({tokens} tok) \u{2500}\u{2500}",
            r.model, r.prompt, r.variant
        ));
        run.log(&format!("\u{2502} 1. Question:        {question}"));
        run.log(&format!("\u{2502} 2. Expected reason: {expected}"));
        let resp = r.agent_response.replace('\n', " ");
        run.log(&format!("\u{2502} 3. Agent response:  {resp}"));
        run.log(&format!(
            "\u{2502} 4. Judge response:  {}",
            r.judge_raw.as_deref().unwrap_or("(none)")
        ));
        run.log(&format!(
            "\u{2502} 5. Result:          Task={t}  Restart={re}  Reason={p}  ({:.0}ms)",
            r.latency_ms
        ));
        run.log("\u{2514}\u{2500}\u{2500}\n");
    }
}

// ---------------------------------------------------------------------------
// Test orchestration
// ---------------------------------------------------------------------------

fn log_precheck_result(precheck: &PrecheckResult) {
    if !precheck.pass {
        eprintln!(
            "
{}",
            "=".repeat(70)
        );
        eprintln!("  PRECHECK FAILED — fix the issues below before running");
        eprintln!(
            "{}
",
            "=".repeat(70)
        );
        for msg in &precheck.errors {
            eprintln!("  ✘ {msg}");
        }
        eprintln!();
        for msg in &precheck.warnings {
            eprintln!("  ⚠ {msg}");
        }
        eprintln!();
        panic!("precheck failed: {} error(s)", precheck.errors.len());
    }
    for msg in &precheck.warnings {
        eprintln!("  ⚠ {msg}");
    }
    for msg in &precheck.ok {
        eprintln!("  ✔ {msg}");
    }
    eprintln!();
}

fn build_judge_cal_config(judge_cfg: &serde_yaml::Value) -> LlmEvaluatorConfig {
    let endpoint = yaml_str(judge_cfg, "endpoint", "judge");
    let model = yaml_str(judge_cfg, "model", "judge");
    let provider = parse_provider(&yaml_str(judge_cfg, "provider", "judge"));
    let api_key = judge_cfg["api_key_env"]
        .as_str()
        .and_then(|env_name| std::env::var(env_name).ok());

    LlmEvaluatorConfig {
        endpoint: endpoint.clone(),
        model: model.clone(),
        provider,
        api_key: api_key.clone(),
        max_tokens: 200,
        temperature: 0.0,
        tls_cert_path: None,
        tls_key_path: None,
        tls_insecure: false,
        judge_endpoint: Some(endpoint),
        judge_model: Some(model),
        judge_provider: Some(provider),
        judge_api_key: api_key,
        semantic_classifier_endpoint: None,
        semantic_classifier_model: None,
        semantic_classifier_provider: None,
        semantic_classifier_api_key: None,
        semantic_classifier_mode: LlmSemanticClassifierMode::ChatCompletions,
    }
}

fn build_agent_cal_config(
    agent_cfg: &serde_yaml::Value,
    tls_section: &serde_yaml::Value,
) -> LlmEvaluatorConfig {
    let tls = agent_cfg["tls"].as_bool().unwrap_or(false);
    let tls_cert_path = if tls {
        Some(yaml_str(tls_section, "cert", "tls"))
    } else {
        None
    };
    let tls_key_path = if tls {
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
            .and_then(|env_name| std::env::var(env_name).ok()),
        max_tokens: agent_cfg["max_tokens"].as_u64().unwrap_or(200) as u32,
        temperature: agent_cfg["temperature"].as_f64().unwrap_or(0.0),
        tls_cert_path,
        tls_key_path,
        tls_insecure: false,
        judge_endpoint: None,
        judge_model: None,
        judge_provider: None,
        judge_api_key: None,
        semantic_classifier_endpoint: None,
        semantic_classifier_model: None,
        semantic_classifier_provider: None,
        semantic_classifier_api_key: None,
        semantic_classifier_mode: LlmSemanticClassifierMode::ChatCompletions,
    }
}

fn log_calibration_cases(
    run: &mut RunDir,
    label: &str,
    name: &str,
    cases: &[kmp_testkit::CalibrationCase],
) -> bool {
    let mut failed = false;
    for case in cases {
        let icon = if case.passed { "✔" } else { "✘" };
        run.log(&format!(
            "  {icon} {}: expected={}, got={}",
            case.name, case.expected, case.got
        ));
        if !case.passed {
            failed = true;
        }
    }
    if failed {
        run.log(&format!(
            "[CALIBRATION] FAILED for {label} '{name}' — aborting to avoid wasting eval budget"
        ));
    }
    failed
}

fn build_scale_entries(
    matrix: &serde_yaml::Value,
) -> Vec<(&'static str, fn(Domain) -> GraphSeedConfig)> {
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
    if yaml_scales.is_empty() {
        all_scales
    } else {
        all_scales
            .into_iter()
            .filter(|(name, _)| yaml_scales.iter().any(|s| s == name))
            .collect()
    }
}

fn noise_modes_for_mix(mix_name: &str) -> &'static [(&'static str, NoiseMode)] {
    match mix_name {
        "explanatory" => &[
            ("clean", NoiseMode::Structural),
            ("competing", NoiseMode::CompetingCausal),
        ],
        "structural" => &[
            ("clean", NoiseMode::Structural),
            ("conflicting", NoiseMode::ConflictingMainPath),
        ],
        "mixed" => &[
            ("clean", NoiseMode::Structural),
            ("restart", NoiseMode::CompetingRestartPoint),
        ],
        _ => &[("clean", NoiseMode::Structural)],
    }
}

struct CaptureJob {
    scale_fn: fn(Domain) -> GraphSeedConfig,
    domain_name: &'static str,
    domain: Domain,
    mix: RelationMix,
    noise_mode: NoiseMode,
    seed_idx: usize,
    variant_id: String,
}

fn capture_jobs(
    matrix: &serde_yaml::Value,
    filter_scales: &[String],
    filter_noise: &[String],
) -> Vec<CaptureJob> {
    let seeds_per_cell = matrix["seeds_per_cell"].as_u64().unwrap_or(1) as usize;
    let scales = build_scale_entries(matrix);
    let domains = [
        ("ops", Domain::Operations),
        ("debug", Domain::SoftwareDebugging),
    ];
    let mixes = [
        ("explanatory", RelationMix::Explanatory),
        ("structural", RelationMix::Structural),
        ("mixed", RelationMix::Mixed),
    ];

    scales
        .into_iter()
        .filter(|(scale_name, _)| filter_allows(scale_name, filter_scales))
        .flat_map(|(scale_name, scale_fn)| {
            domains.into_iter().flat_map(move |(domain_name, domain)| {
                mixes.into_iter().flat_map(move |(mix_name, mix)| {
                    noise_modes_for_mix(mix_name)
                        .iter()
                        .copied()
                        .filter(move |(noise_name, _)| filter_allows(noise_name, filter_noise))
                        .flat_map(move |(noise_name, noise_mode)| {
                            (0..seeds_per_cell).map(move |seed_idx| {
                                let variant_id = if seeds_per_cell > 1 {
                                    format!(
                                        "{scale_name}-{domain_name}-{mix_name}-{noise_name}-s{seed_idx}"
                                    )
                                } else {
                                    format!("{scale_name}-{domain_name}-{mix_name}-{noise_name}")
                                };
                                CaptureJob {
                                    scale_fn,
                                    domain_name,
                                    domain,
                                    mix,
                                    noise_mode,
                                    seed_idx,
                                    variant_id,
                                }
                            })
                        })
                })
            })
        })
        .collect()
}

async fn execute_capture_job(
    run: &mut RunDir,
    fixture: &mut Option<TestFixture>,
    job: &CaptureJob,
) -> Result<CapturedVariant, Box<dyn Error + Send + Sync>> {
    let mut config = (job.scale_fn)(job.domain);
    config.relation_mix = job.mix;
    config.noise_mode = job.noise_mode;
    config.seed = job.seed_idx;
    config.id_prefix = job.variant_id.clone();

    let seed = generate_seed(config.clone());
    let events = seed_to_projection_events(&seed, SUBJECT_PREFIX, &job.variant_id)?;
    let root_id = seed.root.node_id.clone();
    let focus_id = seed
        .nodes
        .first()
        .map(|n| n.node_id.clone())
        .unwrap_or_else(|| root_id.clone());

    match fixture {
        None => {
            run.log(&format!(
                "[INFRA] Booting containers for {}...",
                job.variant_id
            ));
            let f = TestFixture::builder()
                .with_neo4j()
                .with_valkey()
                .with_nats()
                .with_projection_runtime()
                .with_grpc_server()
                .build()
                .await?;
            let client = connect_nats_with_retry(f.nats_url()).await?;
            for (subject, payload) in &events {
                client
                    .publish(subject.clone(), payload.clone().into())
                    .await?;
            }
            client.flush().await?;
            wait_for_context(&f, &root_id, &focus_id).await?;
            *fixture = Some(f);
        }
        Some(f) => {
            run.log(&format!("[INFRA] Reseeding {}...", job.variant_id));
            let client = connect_nats_with_retry(f.nats_url()).await?;
            for (subject, payload) in &events {
                client
                    .publish(subject.clone(), payload.clone().into())
                    .await?;
            }
            client.flush().await?;
            wait_for_context(f, &root_id, &focus_id).await?;
        }
    }

    sleep(Duration::from_millis(500)).await;

    let mut qc = fixture
        .as_ref()
        .expect("fixture should be Some")
        .query_client();
    let response = qc
        .get_context(GetContextRequest {
            root_node_id: root_id.clone(),
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

    let rendered = response.rendered.ok_or("missing rendered")?;
    let eval_content = tier_content_for_eval(&rendered);
    let tokens = rendered.token_count;
    let quality = rendered
        .quality
        .ok_or("missing quality metrics in rendered context")?;
    let (question, ground_truth) = build_ground_truth_for_seed(&seed, job.domain_name);
    let chain_kinds: String = seed
        .nodes
        .iter()
        .filter(|n| !n.node_id.contains("noise") && !n.node_id.contains("distractor"))
        .map(|n| n.node_kind.as_str())
        .collect::<Vec<_>>()
        .join("→");
    run.log(&format!(
        "[CAPTURE] {}: {tokens} tok (raw={}, compress={:.2}x, causal={:.0}%, noise={:.0}%, detail={:.0}%), chain=[{chain_kinds}], reason={}, distractor={}",
        job.variant_id,
        quality.raw_equivalent_tokens,
        quality.compression_ratio,
        quality.causal_density * 100.0,
        quality.noise_ratio * 100.0,
        quality.detail_coverage * 100.0,
        ground_truth.expected_reason.as_deref().unwrap_or("none"),
        ground_truth.distractor_rationale.as_deref().unwrap_or("none")
    ));

    let timing = response.timing.as_ref();
    let obs = extract_kernel_observability(
        &rendered,
        timing.map(|t| t.graph_load_seconds * 1000.0).unwrap_or(0.0),
        timing
            .map(|t| t.detail_load_seconds * 1000.0)
            .unwrap_or(0.0),
        timing
            .map(|t| t.bundle_assembly_seconds * 1000.0)
            .unwrap_or(0.0),
        timing.map(|t| t.batch_size).unwrap_or(0),
    );

    Ok(CapturedVariant {
        run_id: job.variant_id.clone(),
        rendered_content: eval_content,
        rendered_tokens: tokens,
        raw_equivalent_tokens: quality.raw_equivalent_tokens,
        compression_ratio: quality.compression_ratio,
        causal_density: quality.causal_density,
        noise_ratio: quality.noise_ratio,
        detail_coverage: quality.detail_coverage,
        resolved_mode: obs.resolved_mode,
        tier_l0_tokens: obs.tier_l0_tokens,
        tier_l1_tokens: obs.tier_l1_tokens,
        tier_l2_tokens: obs.tier_l2_tokens,
        tier_total_tokens: obs.tier_total_tokens,
        graph_load_ms: obs.graph_load_ms,
        detail_load_ms: obs.detail_load_ms,
        bundle_assembly_ms: obs.bundle_assembly_ms,
        timing_batch_size: obs.timing_batch_size,
        question,
        ground_truth,
    })
}

async fn run_judge_calibrations(
    run: &mut RunDir,
    matrix: &serde_yaml::Value,
    filter_judges: &[String],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let judges = matrix["judges"].as_mapping().expect("judges mapping");
    let default_prompt = PromptConfig::load(None)?;

    for (judge_key, judge_cfg) in judges {
        let judge_name = judge_key.as_str().expect("judge key");
        if !filter_allows(judge_name, filter_judges) {
            continue;
        }
        let cal_config = build_judge_cal_config(judge_cfg);
        run.log(&format!(
            "
[CALIBRATION] Testing judge '{judge_name}' with known-good/known-bad cases..."
        ));
        let cases = calibrate_judge(&default_prompt, &cal_config).await?;
        if log_calibration_cases(run, "judge", judge_name, &cases) {
            panic!(
                "judge calibration failed for '{judge_name}': judge is miscalibrated, fix prompt or switch model"
            );
        }
        run.log(&format!(
            "[CALIBRATION] Judge '{judge_name}' passed ({} cases)
",
            cases.len()
        ));
    }

    Ok(())
}

async fn run_agent_calibrations(
    run: &mut RunDir,
    matrix: &serde_yaml::Value,
    filter_models: &[String],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let agents_map = matrix["agents"].as_mapping().expect("agents mapping");
    let tls_section = &matrix["tls"];

    for (agent_key, agent_cfg) in agents_map {
        let agent_name = agent_key.as_str().expect("agent key");
        if !filter_allows(agent_name, filter_models) {
            continue;
        }
        let agent_cal_config = build_agent_cal_config(agent_cfg, tls_section);
        run.log(&format!(
            "
[CALIBRATION] Testing agent '{agent_name}' inference..."
        ));
        let cases = calibrate_agent(&agent_cal_config).await?;
        if log_calibration_cases(run, "agent", agent_name, &cases) {
            panic!(
                "agent calibration failed for '{agent_name}': model returns empty or malformed responses. Check LLM_ENABLE_THINKING vs --reasoning-parser on the vLLM server."
            );
        }
        run.log(&format!(
            "[CALIBRATION] Agent '{agent_name}' passed ({} cases)
",
            cases.len()
        ));
    }

    Ok(())
}

async fn run_capture_phase(
    run: &mut RunDir,
    matrix: &serde_yaml::Value,
    filter_scales: &[String],
    filter_noise: &[String],
) -> Result<(Vec<CapturedVariant>, f64), Box<dyn Error + Send + Sync>> {
    let jobs = capture_jobs(matrix, filter_scales, filter_noise);
    let active_scale_names = build_scale_entries(matrix)
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>();
    run.log(&format!(
        "[CONFIG] seeds_per_cell={}, scales={active_scale_names:?}",
        matrix["seeds_per_cell"].as_u64().unwrap_or(1)
    ));
    let boot_start = Instant::now();
    let mut captured = Vec::new();
    let mut fixture: Option<TestFixture> = None;

    for job in jobs {
        captured.push(execute_capture_job(run, &mut fixture, &job).await?);
    }

    Ok((captured, boot_start.elapsed().as_secs_f64() * 1000.0))
}

struct EvaluationJob {
    agent_name: String,
    judge_name: String,
    prompt_name: String,
    prompts_cfg: PromptConfig,
    agent_cfg: serde_yaml::Value,
    judge_cfg: serde_yaml::Value,
}

fn evaluation_jobs(
    matrix: &serde_yaml::Value,
    resources: &str,
    filter_models: &[String],
    filter_judges: &[String],
    filter_prompts: &[String],
) -> Result<Vec<EvaluationJob>, Box<dyn Error + Send + Sync>> {
    let agents = matrix["agents"].as_mapping().expect("agents mapping");
    let judges = matrix["judges"].as_mapping().expect("judges mapping");
    let prompts = matrix["prompts"].as_mapping().expect("prompts mapping");

    let mut jobs = Vec::new();
    for (agent_key, agent_cfg) in agents {
        let agent_name = agent_key.as_str().expect("agent key");
        if !filter_allows(agent_name, filter_models) {
            continue;
        }
        for (judge_key, judge_cfg) in judges {
            let judge_name = judge_key.as_str().expect("judge key");
            if !filter_allows(judge_name, filter_judges) || agent_name == judge_name {
                continue;
            }
            for (prompt_key, prompt_val) in prompts {
                let prompt_name = prompt_key.as_str().expect("prompt key");
                if !filter_allows(prompt_name, filter_prompts) {
                    continue;
                }
                let prompt_file = prompt_val.as_str().map(|p| format!("{resources}/{p}"));
                let prompts_cfg = PromptConfig::load(prompt_file.as_deref())?;
                jobs.push(EvaluationJob {
                    agent_name: agent_name.to_string(),
                    judge_name: judge_name.to_string(),
                    prompt_name: prompt_name.to_string(),
                    prompts_cfg,
                    agent_cfg: agent_cfg.clone(),
                    judge_cfg: judge_cfg.clone(),
                });
            }
        }
    }
    Ok(jobs)
}

fn build_success_eval_result(
    ctx: &CapturedVariant,
    cell_id: String,
    prompt_name: &str,
    e: LlmEvaluationResult,
) -> EvalResult {
    EvalResult {
        model: cell_id,
        prompt: prompt_name.to_string(),
        variant: ctx.run_id.clone(),
        task: Some(e.llm_task_success),
        restart: Some(e.llm_restart_accuracy),
        restart_exact: Some(e.llm_restart_exact),
        restart_off_by_one: Some(e.llm_restart_off_by_one),
        restart_on_competing: Some(e.llm_restart_on_competing),
        restart_explained: Some(e.llm_restart_explained),
        reason: Some(e.llm_reason_preserved),
        reason_correct: Some(e.llm_reason_correct),
        reason_distractor: Some(e.llm_reason_distractor),
        latency_ms: e.llm_latency_ms,
        rendered_tokens: ctx.rendered_tokens,
        raw_equivalent_tokens: ctx.raw_equivalent_tokens,
        compression_ratio: ctx.compression_ratio,
        causal_density: ctx.causal_density,
        noise_ratio: ctx.noise_ratio,
        detail_coverage: ctx.detail_coverage,
        resolved_mode: ctx.resolved_mode.clone(),
        tier_l0_tokens: ctx.tier_l0_tokens,
        tier_l1_tokens: ctx.tier_l1_tokens,
        tier_l2_tokens: ctx.tier_l2_tokens,
        tier_total_tokens: ctx.tier_total_tokens,
        graph_load_ms: ctx.graph_load_ms,
        detail_load_ms: ctx.detail_load_ms,
        bundle_assembly_ms: ctx.bundle_assembly_ms,
        timing_batch_size: ctx.timing_batch_size,
        llm_prompt_tokens: Some(e.llm_prompt_tokens),
        llm_completion_tokens: Some(e.llm_completion_tokens),
        llm_reason_source: Some(e.llm_reason_source),
        llm_confidence: Some(e.llm_confidence),
        llm_reason_fabricated: Some(e.llm_reason_fabricated),
        agent_response: e.llm_response,
        judge_raw: e.llm_judge_raw,
    }
}

async fn evaluate_job(
    run: &mut RunDir,
    matrix: &serde_yaml::Value,
    captured: &[CapturedVariant],
    job: &EvaluationJob,
) -> Result<Vec<EvalResult>, Box<dyn Error + Send + Sync>> {
    let llm_config = build_llm_config(matrix, &job.agent_cfg, &job.judge_cfg);
    run.log(&format!(
        "\n████ {}→{}/{} ████",
        job.agent_name, job.judge_name, job.prompt_name
    ));

    let mut results = Vec::new();
    for ctx in captured {
        let eval = evaluate_with_config(
            &job.prompts_cfg,
            &llm_config,
            &ctx.rendered_content,
            &ctx.question,
            &ctx.ground_truth,
        )
        .await;
        let cell_id = format!("{}→{}", job.agent_name, job.judge_name);
        let result = match eval {
            Ok(e) => {
                run.log(&format!(
                    "  [{cell_id}/{}] {}: T={} R={} (x={} o1={} cb={} ex={}) Rc={} Rd={}  agent=\"{}...\"  judge={}",
                    job.prompt_name,
                    ctx.run_id,
                    if e.llm_task_success { "OK" } else { "FAIL" },
                    if e.llm_restart_accuracy { "OK" } else { "FAIL" },
                    if e.llm_restart_exact { "OK" } else { "FAIL" },
                    if e.llm_restart_off_by_one { "OK" } else { "FAIL" },
                    if e.llm_restart_on_competing { "OK" } else { "FAIL" },
                    if e.llm_restart_explained { "OK" } else { "FAIL" },
                    if e.llm_reason_correct { "OK" } else { "FAIL" },
                    if e.llm_reason_distractor { "LEAK" } else { "clean" },
                    e.llm_response.replace('\n', " ").chars().take(120).collect::<String>(),
                    e.llm_judge_raw.as_deref().unwrap_or("?")
                ));
                build_success_eval_result(ctx, cell_id, &job.prompt_name, e)
            }
            Err(err) => {
                run.log(&format!(
                    "  [{cell_id}/{}] {}: ERROR {err}",
                    job.prompt_name, ctx.run_id
                ));
                build_eval_result_from_ctx(ctx, cell_id, &job.prompt_name)
            }
        };
        results.push(result);
    }

    Ok(results)
}

async fn run_evaluation_phase(
    run: &mut RunDir,
    matrix: &serde_yaml::Value,
    resources: &str,
    captured: &[CapturedVariant],
    filter_models: &[String],
    filter_judges: &[String],
    filter_prompts: &[String],
) -> Result<Vec<EvalResult>, Box<dyn Error + Send + Sync>> {
    let jobs = evaluation_jobs(
        matrix,
        resources,
        filter_models,
        filter_judges,
        filter_prompts,
    )?;
    let mut results = Vec::new();
    let mut eval_count = 0u32;

    for job in jobs {
        let job_results = evaluate_job(run, matrix, captured, &job).await?;
        for result in job_results {
            eval_count += 1;
            let result_name = format!(
                "{eval_count:04}_{}_{}_{}_{}",
                job.agent_name, job.judge_name, job.prompt_name, result.variant
            );
            run.write_result(&result_name, &result)?;
            results.push(result);
        }
    }

    Ok(results)
}

async fn run_judge_prompt_evaluation() -> Result<(), Box<dyn Error + Send + Sync>> {
    let resources = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/kmp-testkit/resources"
    );
    let matrix_path = std::env::var("EVAL_MATRIX_PATH")
        .unwrap_or_else(|_| format!("{resources}/evaluation-matrix.yaml"));

    let precheck = precheck(resources, &matrix_path);
    log_precheck_result(&precheck);

    let mut run = RunDir::create()?;
    let boot_start = Instant::now();

    let matrix: serde_yaml::Value = serde_yaml::from_str(&std::fs::read_to_string(&matrix_path)?)?;
    run.log(&format!("[CONFIG] matrix={matrix_path}"));

    let filter_models = env_filter("FILTER_MODELS");
    let filter_prompts = env_filter("FILTER_PROMPTS");
    let filter_scales = env_filter("FILTER_SCALES");
    let filter_noise = env_filter("FILTER_NOISE");
    let filter_judges = env_filter("FILTER_JUDGES");

    run_judge_calibrations(&mut run, &matrix, &filter_judges).await?;
    run_agent_calibrations(&mut run, &matrix, &filter_models).await?;

    let (captured, boot_ms) =
        run_capture_phase(&mut run, &matrix, &filter_scales, &filter_noise).await?;
    run.log(&format!(
        "[INFRA] {} variants captured in {boot_ms:.0}ms
",
        captured.len()
    ));

    let results = run_evaluation_phase(
        &mut run,
        &matrix,
        resources,
        &captured,
        &filter_models,
        &filter_judges,
        &filter_prompts,
    )
    .await?;

    let total_ms = boot_start.elapsed().as_secs_f64() * 1000.0;
    write_report(&mut run, &results, &captured, boot_ms, total_ms);
    Ok(())
}

#[tokio::test]
async fn judge_prompt_evaluation_across_all_use_cases() -> Result<(), Box<dyn Error + Send + Sync>>
{
    run_judge_prompt_evaluation().await
}
