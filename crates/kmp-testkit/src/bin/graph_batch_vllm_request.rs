use std::env;
use std::error::Error;
use std::fs;
use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

use kmp_testkit::{
    GraphBatchRepairJudgePolicy, GraphBatchRetryPolicy, GraphBatchSemanticClassifierPolicy,
    LlmEvaluatorConfig, LlmProvider, build_graph_batch_request_body,
    classify_graph_batch_semantic_classes_with_policy, namespace_graph_batch,
    request_graph_batch_with_policy, request_graph_batch_with_repair_judge,
};

const DEFAULT_REQUEST_FIXTURE: &str = include_str!(
    "../../../../api/examples/inference-prompts/vllm-graph-materialization.request.json"
);
const BLIND_REQUEST_FIXTURE: &str = include_str!(
    "../../../../api/examples/inference-prompts/vllm-graph-materialization.blind.request.json"
);
const LARGE_INCIDENT_REQUEST_FIXTURE: &str = include_str!(
    "../../../../api/examples/inference-prompts/vllm-graph-materialization.large-incident.request.json"
);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    request_fixture: Option<String>,
    request_kind: RequestKind,
    subject_prefix: String,
    run_id: String,
    use_repair_judge: bool,
    use_semantic_classifier: bool,
    namespace_node_ids: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequestKind {
    Default,
    Blind,
    LargeIncident,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = parse_args(env::args().skip(1))?;
    let config = LlmEvaluatorConfig::from_env();
    if !matches!(
        config.provider,
        LlmProvider::OpenAI | LlmProvider::OpenAINew
    ) {
        return Err(format!(
            "graph_batch_vllm_request requires an OpenAI-compatible provider, got {:?}",
            config.provider
        )
        .into());
    }

    let request_fixture = read_request_fixture(args.request_fixture.as_deref(), args.request_kind)?;
    let request_body = build_graph_batch_request_body(&config, &request_fixture)?;
    let primary_policy = GraphBatchRetryPolicy::from_env();
    let outcome = if args.use_repair_judge {
        request_graph_batch_with_repair_judge(
            &config,
            request_body,
            &args.subject_prefix,
            &args.run_id,
            primary_policy,
            GraphBatchRepairJudgePolicy::from_env(),
        )
        .await?
    } else {
        request_graph_batch_with_policy(
            &config,
            request_body,
            &args.subject_prefix,
            &args.run_id,
            primary_policy,
        )
        .await?
    };
    let mut batch = outcome.batch;
    let mut semantic_classifier_attempts = 0;
    let mut semantic_classifier_changed_relations = 0;
    if args.use_semantic_classifier {
        let classified = classify_graph_batch_semantic_classes_with_policy(
            &config,
            &batch,
            &args.subject_prefix,
            &args.run_id,
            GraphBatchSemanticClassifierPolicy::from_env(),
        )
        .await?;
        semantic_classifier_attempts = classified.attempts;
        semantic_classifier_changed_relations = classified.changed_relations;
        batch = classified.batch;
    }
    if args.namespace_node_ids {
        namespace_graph_batch(&mut batch, &args.run_id);
    }

    eprintln!(
        "generated GraphBatch root={} nodes={} relations={} details={} primary_attempts={} repair_attempts={} repaired_by_judge={} semantic_classifier={} semantic_classifier_attempts={} semantic_classifier_changed_relations={} namespace_node_ids={}",
        batch.root_node_id,
        batch.nodes.len(),
        batch.relations.len(),
        batch.node_details.len(),
        outcome.primary_attempts,
        outcome.repair_attempts,
        outcome.repaired_by_judge,
        args.use_semantic_classifier,
        semantic_classifier_attempts,
        semantic_classifier_changed_relations,
        args.namespace_node_ids
    );
    println!("{}", serde_json::to_string_pretty(&batch)?);

    Ok(())
}

fn read_request_fixture(
    path: Option<&str>,
    request_kind: RequestKind,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    match path {
        Some(path) => Ok(fs::read_to_string(path)?),
        None => Ok(match request_kind {
            RequestKind::Default => DEFAULT_REQUEST_FIXTURE,
            RequestKind::Blind => BLIND_REQUEST_FIXTURE,
            RequestKind::LargeIncident => LARGE_INCIDENT_REQUEST_FIXTURE,
        }
        .to_string()),
    }
}

fn parse_args(
    mut args: impl Iterator<Item = String>,
) -> Result<Args, Box<dyn Error + Send + Sync>> {
    let mut request_fixture = None;
    let mut request_kind = RequestKind::Default;
    let mut subject_prefix = "rehydration".to_string();
    let mut run_id = None;
    let mut use_repair_judge = env::var("LLM_GRAPH_BATCH_USE_REPAIR_JUDGE")
        .map(|value| value == "true" || value == "1")
        .unwrap_or(false);
    let mut use_semantic_classifier = env::var("LLM_GRAPH_BATCH_USE_SEMANTIC_CLASSIFIER")
        .map(|value| value == "true" || value == "1")
        .unwrap_or(false);
    let mut namespace_node_ids = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--request-fixture" => request_fixture = Some(required_flag_value(&mut args, &arg)?),
            "--blind" => request_kind = RequestKind::Blind,
            "--large-incident" => request_kind = RequestKind::LargeIncident,
            "--subject-prefix" => subject_prefix = required_flag_value(&mut args, &arg)?,
            "--run-id" => run_id = Some(required_flag_value(&mut args, &arg)?),
            "--use-repair-judge" => use_repair_judge = true,
            "--use-semantic-classifier" => use_semantic_classifier = true,
            "--namespace-node-ids" => namespace_node_ids = true,
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => {
                return Err(Box::new(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown argument `{other}`"),
                )));
            }
        }
    }

    Ok(Args {
        request_fixture,
        request_kind,
        subject_prefix,
        run_id: run_id.unwrap_or_else(|| format!("vllm-graph-{}", unix_timestamp_secs())),
        use_repair_judge,
        use_semantic_classifier,
        namespace_node_ids,
    })
}

fn required_flag_value(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    args.next().ok_or_else(|| {
        Box::new(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{flag} requires a value"),
        )) as Box<dyn Error + Send + Sync>
    })
}

fn unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn print_usage() {
    eprintln!(
        "usage: graph_batch_vllm_request [--request-fixture <path>|--blind|--large-incident] [--subject-prefix <prefix>] [--run-id <id>] [--use-repair-judge] [--use-semantic-classifier] [--namespace-node-ids]"
    );
}

#[cfg(test)]
mod tests {
    use super::{Args, RequestKind, parse_args};

    #[test]
    fn parse_args_uses_defaults() {
        let args = parse_args([].into_iter()).expect("args should parse");

        assert_eq!(args.request_fixture, None);
        assert_eq!(args.request_kind, RequestKind::Default);
        assert_eq!(args.subject_prefix, "rehydration");
        assert!(args.run_id.starts_with("vllm-graph-"));
        assert!(!args.use_repair_judge);
        assert!(!args.use_semantic_classifier);
        assert!(!args.namespace_node_ids);
    }

    #[test]
    fn parse_args_reads_overrides() {
        let args = parse_args(
            [
                "--request-fixture",
                "request.json",
                "--blind",
                "--large-incident",
                "--subject-prefix",
                "custom",
                "--run-id",
                "run-1",
                "--use-repair-judge",
                "--use-semantic-classifier",
                "--namespace-node-ids",
            ]
            .into_iter()
            .map(ToString::to_string),
        )
        .expect("args should parse");

        assert_eq!(
            args,
            Args {
                request_fixture: Some("request.json".to_string()),
                request_kind: RequestKind::LargeIncident,
                subject_prefix: "custom".to_string(),
                run_id: "run-1".to_string(),
                use_repair_judge: true,
                use_semantic_classifier: true,
                namespace_node_ids: true,
            }
        );
    }

    #[test]
    fn parse_args_supports_blind_fixture() {
        let args = parse_args(["--blind"].into_iter().map(ToString::to_string))
            .expect("args should parse");

        assert_eq!(args.request_fixture, None);
        assert_eq!(args.request_kind, RequestKind::Blind);
    }
}
