use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::fs;
use std::io;
use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use async_nats::{Client, ConnectOptions};
use kmp_proto::v1beta1::{
    GetContextRequest, GetContextResponse, GetNodeDetailRequest, GetNodeDetailResponse,
    GraphRelationshipSemanticClass, context_query_service_client::ContextQueryServiceClient,
};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::time::sleep;
use tonic::Code;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity};

type AppError = Box<dyn Error + Send + Sync>;
type ProjectionMessage = (String, Vec<u8>);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    input: String,
    nats_url: String,
    grpc_endpoint: String,
    subject_prefix: String,
    run_id: String,
    role: String,
    requested_scopes: Vec<String>,
    depth: u32,
    token_budget: u32,
    rehydration_mode: i32,
    detail_node_id: Option<String>,
    wait_timeout_secs: u64,
    poll_interval_ms: u64,
    grpc_tls_ca_path: Option<String>,
    grpc_tls_cert_path: Option<String>,
    grpc_tls_key_path: Option<String>,
    grpc_tls_domain_name: Option<String>,
    nats_tls_ca_path: Option<String>,
    nats_tls_cert_path: Option<String>,
    nats_tls_key_path: Option<String>,
    nats_tls_first: bool,
    include_rendered_content: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct RelationRoundtripFixture {
    root_node_id: String,
    #[serde(default)]
    detail_node_id: Option<String>,
    events: Vec<ProjectionEventFixture>,
}

#[derive(Debug, Clone, Deserialize)]
struct ProjectionEventFixture {
    subject: String,
    payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExpectedProjection {
    node_ids: Vec<String>,
    relations: Vec<ExpectedRelation>,
    details: Vec<ExpectedDetail>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExpectedRelation {
    source_node_id: String,
    target_node_id: String,
    relation_type: String,
    semantic_class: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExpectedDetail {
    node_id: String,
    detail: String,
    revision: u64,
}

#[derive(Debug, Serialize)]
struct RoundtripSummary {
    root_node_id: String,
    run_id: String,
    published_messages: usize,
    subject_prefix: String,
    grpc_endpoint: String,
    nats_url: String,
    role: String,
    requested_scopes: Vec<String>,
    depth: u32,
    token_budget: u32,
    rehydration_mode: i32,
    selected_detail_node_id: Option<String>,
    bundle_role_count: usize,
    neighbor_count: usize,
    relationship_count: usize,
    detail_count: usize,
    rendered_chars: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    rendered_content: Option<String>,
    detail_revision: Option<u64>,
    detail_excerpt: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    install_crypto_provider();
    let args = parse_args(env::args().skip(1))?;
    let payload = substitute_run_id(&read_input_payload(&args.input)?, &args.run_id);
    let fixture: RelationRoundtripFixture = serde_json::from_str(&payload)?;
    let expected = expected_projection_from_fixture(&fixture)?;
    let selected_detail_node_id = select_detail_node_id(&args, &fixture, &expected);
    let messages = projection_messages(&fixture, &args.subject_prefix)?;

    let nats_client = connect_nats(&args).await?;
    publish_messages(&nats_client, &messages).await?;

    let mut query_client = connect_query_client(&args).await?;
    let (context, detail) = wait_for_projection(
        &mut query_client,
        &fixture,
        &expected,
        &args,
        selected_detail_node_id.as_deref(),
    )
    .await?;

    let summary = build_summary(
        &fixture,
        &args,
        messages.len(),
        selected_detail_node_id,
        context,
        detail,
    );

    println!("{}", serde_json::to_string_pretty(&summary)?);

    Ok(())
}

fn read_input_payload(input: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
    if input == "-" {
        let mut payload = String::new();
        io::stdin().read_to_string(&mut payload)?;
        return Ok(payload);
    }

    Ok(fs::read_to_string(input)?)
}

fn substitute_run_id(payload: &str, run_id: &str) -> String {
    payload.replace("{{RUN_ID}}", run_id)
}

fn projection_messages(
    fixture: &RelationRoundtripFixture,
    subject_prefix: &str,
) -> Result<Vec<ProjectionMessage>, AppError> {
    fixture
        .events
        .iter()
        .map(|event| {
            Ok((
                format!("{subject_prefix}.{}", event.subject),
                serde_json::to_vec(&event.payload)?,
            ))
        })
        .collect()
}

fn expected_projection_from_fixture(
    fixture: &RelationRoundtripFixture,
) -> Result<ExpectedProjection, Box<dyn Error + Send + Sync>> {
    let mut node_ids = BTreeSet::new();
    let mut relations = BTreeMap::<(String, String, String), ExpectedRelation>::new();
    let mut details = BTreeMap::<String, ExpectedDetail>::new();

    for event in &fixture.events {
        match event.subject.as_str() {
            "graph.node.materialized" => {
                let data = object_field(&event.payload, "data")?;
                let source_node_id = string_field(data, "node_id")?.to_string();
                node_ids.insert(source_node_id.clone());

                if let Some(related_nodes) = data.get("related_nodes").and_then(Value::as_array) {
                    for related_node in related_nodes {
                        let related_node = related_node.as_object().ok_or_else(|| {
                            Box::new(io::Error::new(
                                io::ErrorKind::InvalidInput,
                                "related_nodes entries must be objects",
                            )) as Box<dyn Error + Send + Sync>
                        })?;
                        let target_node_id = string_field(related_node, "node_id")?.to_string();
                        let relation_type =
                            string_field(related_node, "relation_type")?.to_string();
                        let semantic_class = related_node
                            .get("explanation")
                            .and_then(|value| value.get("semantic_class"))
                            .and_then(Value::as_str)
                            .map(parse_semantic_class)
                            .transpose()?;
                        relations.insert(
                            (
                                source_node_id.clone(),
                                target_node_id.clone(),
                                relation_type.clone(),
                            ),
                            ExpectedRelation {
                                source_node_id: source_node_id.clone(),
                                target_node_id,
                                relation_type,
                                semantic_class,
                            },
                        );
                    }
                }
            }
            "graph.relation.materialized" => {
                let data = object_field(&event.payload, "data")?;
                let source_node_id = string_field(data, "source_node_id")?.to_string();
                let target_node_id = string_field(data, "target_node_id")?.to_string();
                let relation_type = string_field(data, "relation_type")?.to_string();
                let semantic_class = data
                    .get("explanation")
                    .and_then(|value| value.get("semantic_class"))
                    .and_then(Value::as_str)
                    .map(parse_semantic_class)
                    .transpose()?;
                relations.insert(
                    (
                        source_node_id.clone(),
                        target_node_id.clone(),
                        relation_type.clone(),
                    ),
                    ExpectedRelation {
                        source_node_id,
                        target_node_id,
                        relation_type,
                        semantic_class,
                    },
                );
            }
            "node.detail.materialized" => {
                let data = object_field(&event.payload, "data")?;
                let node_id = string_field(data, "node_id")?.to_string();
                let detail = string_field(data, "detail")?.to_string();
                let revision = u64_field(data, "revision")?;
                details.insert(
                    node_id.clone(),
                    ExpectedDetail {
                        node_id,
                        detail,
                        revision,
                    },
                );
            }
            other => {
                return Err(Box::new(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unsupported projection subject `{other}`"),
                )));
            }
        }
    }

    Ok(ExpectedProjection {
        node_ids: node_ids.into_iter().collect(),
        relations: relations.into_values().collect(),
        details: details.into_values().collect(),
    })
}

fn build_summary(
    fixture: &RelationRoundtripFixture,
    args: &Args,
    published_messages: usize,
    selected_detail_node_id: Option<String>,
    context: GetContextResponse,
    detail: Option<GetNodeDetailResponse>,
) -> RoundtripSummary {
    let bundle = context.bundle.unwrap_or_default();
    let role_bundle = bundle.bundles.first();
    let rendered = context.rendered.unwrap_or_default();

    let detail_revision = detail
        .as_ref()
        .and_then(|response| response.detail.as_ref())
        .map(|detail| detail.revision);
    let detail_excerpt = detail
        .as_ref()
        .and_then(|response| response.detail.as_ref())
        .map(|detail| excerpt(&detail.detail, 180));

    RoundtripSummary {
        root_node_id: fixture.root_node_id.clone(),
        run_id: args.run_id.clone(),
        published_messages,
        subject_prefix: args.subject_prefix.clone(),
        grpc_endpoint: args.grpc_endpoint.clone(),
        nats_url: args.nats_url.clone(),
        role: args.role.clone(),
        requested_scopes: args.requested_scopes.clone(),
        depth: args.depth,
        token_budget: args.token_budget,
        rehydration_mode: args.rehydration_mode,
        selected_detail_node_id,
        bundle_role_count: bundle.bundles.len(),
        neighbor_count: role_bundle
            .map(|bundle| bundle.neighbor_nodes.len())
            .unwrap_or(0),
        relationship_count: role_bundle
            .map(|bundle| bundle.relationships.len())
            .unwrap_or(0),
        detail_count: role_bundle
            .map(|bundle| bundle.node_details.len())
            .unwrap_or(0),
        rendered_chars: rendered.content.chars().count(),
        rendered_content: args.include_rendered_content.then_some(rendered.content),
        detail_revision,
        detail_excerpt,
    }
}

async fn publish_messages(
    client: &Client,
    messages: &[(String, Vec<u8>)],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    for (subject, payload) in messages {
        client
            .publish(subject.clone(), payload.clone().into())
            .await?;
    }
    client.flush().await?;
    Ok(())
}

async fn wait_for_projection(
    query_client: &mut ContextQueryServiceClient<Channel>,
    fixture: &RelationRoundtripFixture,
    expected: &ExpectedProjection,
    args: &Args,
    detail_node_id: Option<&str>,
) -> Result<(GetContextResponse, Option<GetNodeDetailResponse>), Box<dyn Error + Send + Sync>> {
    let deadline = Instant::now() + Duration::from_secs(args.wait_timeout_secs);
    let poll_interval = Duration::from_millis(args.poll_interval_ms);
    let mut last_not_ready_reason = None;

    loop {
        match query_client
            .get_context(GetContextRequest {
                root_node_id: fixture.root_node_id.clone(),
                role: args.role.clone(),
                token_budget: args.token_budget,
                requested_scopes: args.requested_scopes.clone(),
                depth: args.depth,
                max_tier: 0,
                rehydration_mode: args.rehydration_mode,
            })
            .await
        {
            Ok(response) => {
                let context = response.into_inner();
                let mut detail_response = None;
                if let Some(node_id) = detail_node_id {
                    match query_client
                        .get_node_detail(GetNodeDetailRequest {
                            node_id: node_id.to_string(),
                        })
                        .await
                    {
                        Ok(detail) => detail_response = Some(detail.into_inner()),
                        Err(status) if should_retry_grpc_status(&status) => {}
                        Err(status) => return Err(Box::new(status)),
                    }
                }

                match projection_matches_expected(
                    &context,
                    detail_response.as_ref(),
                    fixture,
                    expected,
                    detail_node_id,
                    expects_bundle_details(args),
                ) {
                    Ok(()) => return Ok((context, detail_response)),
                    Err(reason) => last_not_ready_reason = Some(reason),
                }
            }
            Err(status) if should_retry_grpc_status(&status) => {}
            Err(status) => return Err(Box::new(status)),
        }

        if Instant::now() >= deadline {
            return Err(Box::new(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "projection did not materialize expected raw relation fixture for root `{}` within {}s: {}",
                    fixture.root_node_id,
                    args.wait_timeout_secs,
                    last_not_ready_reason
                        .as_deref()
                        .unwrap_or("no queryable context was returned")
                ),
            )));
        }

        sleep(poll_interval).await;
    }
}

fn projection_matches_expected(
    context: &GetContextResponse,
    detail: Option<&GetNodeDetailResponse>,
    fixture: &RelationRoundtripFixture,
    expected: &ExpectedProjection,
    detail_node_id: Option<&str>,
    expect_bundle_details: bool,
) -> Result<(), String> {
    let bundle = context
        .bundle
        .as_ref()
        .ok_or_else(|| "GetContext response did not include a bundle".to_string())?;
    if bundle.root_node_id != fixture.root_node_id {
        return Err(format!(
            "bundle root_node_id `{}` did not match expected `{}`",
            bundle.root_node_id, fixture.root_node_id
        ));
    }

    let role_bundle = bundle
        .bundles
        .first()
        .ok_or_else(|| "bundle did not include a role bundle".to_string())?;

    for expected_node_id in &expected.node_ids {
        let found_root = role_bundle
            .root_node
            .as_ref()
            .is_some_and(|actual| actual.node_id == *expected_node_id);
        let found_neighbor = role_bundle
            .neighbor_nodes
            .iter()
            .any(|actual| actual.node_id == *expected_node_id);
        if !found_root && !found_neighbor {
            return Err(format!(
                "missing projected node `{expected_node_id}` in GetContext bundle"
            ));
        }
    }

    for expected_relation in &expected.relations {
        let found = role_bundle.relationships.iter().any(|actual| {
            actual.source_node_id == expected_relation.source_node_id
                && actual.target_node_id == expected_relation.target_node_id
                && actual.relationship_type == expected_relation.relation_type
                && match expected_relation.semantic_class {
                    Some(expected_class) => actual
                        .explanation
                        .as_ref()
                        .is_some_and(|explanation| explanation.semantic_class == expected_class),
                    None => true,
                }
        });
        if !found {
            return Err(format!(
                "missing projected relation `{} -> {} ({})` in GetContext bundle",
                expected_relation.source_node_id,
                expected_relation.target_node_id,
                expected_relation.relation_type
            ));
        }
    }

    if expect_bundle_details {
        for expected_detail in &expected.details {
            let actual = role_bundle
                .node_details
                .iter()
                .find(|actual| actual.node_id == expected_detail.node_id)
                .ok_or_else(|| {
                    format!(
                        "missing projected detail for node `{}` in GetContext bundle",
                        expected_detail.node_id
                    )
                })?;
            detail_matches_expected(actual.detail.as_str(), actual.revision, expected_detail)?;
        }
    }

    if let Some(node_id) = detail_node_id {
        let actual_detail = detail
            .and_then(|response| response.detail.as_ref())
            .ok_or_else(|| format!("GetNodeDetail did not return detail for `{node_id}`"))?;
        if let Some(expected_detail) = expected.details.iter().find(|item| item.node_id == node_id)
        {
            detail_matches_expected(
                actual_detail.detail.as_str(),
                actual_detail.revision,
                expected_detail,
            )?;
        }
    }

    Ok(())
}

fn expects_bundle_details(args: &Args) -> bool {
    args.requested_scopes
        .iter()
        .any(|scope| scope.eq_ignore_ascii_case("details"))
}

fn detail_matches_expected(
    actual_detail: &str,
    actual_revision: u64,
    expected_detail: &ExpectedDetail,
) -> Result<(), String> {
    if actual_detail != expected_detail.detail {
        return Err(format!(
            "detail for node `{}` did not match the published payload",
            expected_detail.node_id
        ));
    }

    if actual_revision != expected_detail.revision {
        return Err(format!(
            "detail revision for node `{}` was {}, expected {}",
            expected_detail.node_id, actual_revision, expected_detail.revision
        ));
    }

    Ok(())
}

fn parse_semantic_class(value: &str) -> Result<i32, Box<dyn Error + Send + Sync>> {
    let semantic_class = match value {
        "unspecified" => GraphRelationshipSemanticClass::Unspecified,
        "structural" => GraphRelationshipSemanticClass::Structural,
        "causal" => GraphRelationshipSemanticClass::Causal,
        "motivational" => GraphRelationshipSemanticClass::Motivational,
        "procedural" => GraphRelationshipSemanticClass::Procedural,
        "evidential" => GraphRelationshipSemanticClass::Evidential,
        "constraint" => GraphRelationshipSemanticClass::Constraint,
        other => {
            return Err(Box::new(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unsupported semantic class `{other}`"),
            )));
        }
    };

    Ok(semantic_class as i32)
}

fn object_field<'a>(
    value: &'a Value,
    key: &str,
) -> Result<&'a serde_json::Map<String, Value>, Box<dyn Error + Send + Sync>> {
    value.get(key).and_then(Value::as_object).ok_or_else(|| {
        Box::new(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("missing object field `{key}`"),
        )) as Box<dyn Error + Send + Sync>
    })
}

fn string_field<'a>(
    object: &'a serde_json::Map<String, Value>,
    key: &str,
) -> Result<&'a str, Box<dyn Error + Send + Sync>> {
    object.get(key).and_then(Value::as_str).ok_or_else(|| {
        Box::new(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("missing string field `{key}`"),
        )) as Box<dyn Error + Send + Sync>
    })
}

fn u64_field(
    object: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<u64, Box<dyn Error + Send + Sync>> {
    object.get(key).and_then(Value::as_u64).ok_or_else(|| {
        Box::new(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("missing integer field `{key}`"),
        )) as Box<dyn Error + Send + Sync>
    })
}

fn should_retry_grpc_status(status: &tonic::Status) -> bool {
    matches!(
        status.code(),
        Code::NotFound | Code::Unavailable | Code::DeadlineExceeded | Code::Unknown
    )
}

async fn connect_query_client(
    args: &Args,
) -> Result<ContextQueryServiceClient<Channel>, Box<dyn Error + Send + Sync>> {
    let endpoint = build_grpc_endpoint(args)?;
    let channel = endpoint.connect().await?;
    Ok(ContextQueryServiceClient::new(channel))
}

fn build_grpc_endpoint(args: &Args) -> Result<Endpoint, Box<dyn Error + Send + Sync>> {
    let mut endpoint = Endpoint::from_shared(args.grpc_endpoint.clone())?;

    if needs_grpc_tls(args) {
        let domain_name = args
            .grpc_tls_domain_name
            .clone()
            .or_else(|| host_from_url(&args.grpc_endpoint))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "gRPC TLS requires a domain name or a host in the endpoint URL",
                )
            })?;

        let mut tls = ClientTlsConfig::new().domain_name(domain_name);

        if let Some(ca_path) = &args.grpc_tls_ca_path {
            tls = tls.ca_certificate(Certificate::from_pem(fs::read(ca_path)?));
        }

        match (&args.grpc_tls_cert_path, &args.grpc_tls_key_path) {
            (Some(cert_path), Some(key_path)) => {
                tls = tls.identity(Identity::from_pem(
                    fs::read(cert_path)?,
                    fs::read(key_path)?,
                ));
            }
            (None, None) => {}
            _ => {
                return Err(Box::new(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "gRPC client identity requires both --grpc-tls-cert-path and --grpc-tls-key-path",
                )));
            }
        }

        endpoint = endpoint.tls_config(tls)?;
    }

    Ok(endpoint)
}

async fn connect_nats(args: &Args) -> Result<Client, Box<dyn Error + Send + Sync>> {
    let mut options = ConnectOptions::new();

    if needs_nats_tls(args) {
        options = options.require_tls(true);

        if let Some(ca_path) = &args.nats_tls_ca_path {
            options = options.add_root_certificates(PathBuf::from(ca_path));
        }

        match (&args.nats_tls_cert_path, &args.nats_tls_key_path) {
            (Some(cert_path), Some(key_path)) => {
                options = options
                    .add_client_certificate(PathBuf::from(cert_path), PathBuf::from(key_path));
            }
            (None, None) => {}
            _ => {
                return Err(Box::new(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "NATS client identity requires both --nats-tls-cert-path and --nats-tls-key-path",
                )));
            }
        }

        if args.nats_tls_first {
            options = options.tls_first();
        }
    } else if args.nats_tls_first {
        return Err(Box::new(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--nats-tls-first requires TLS",
        )));
    }

    Ok(options.connect(&args.nats_url).await?)
}

fn needs_grpc_tls(args: &Args) -> bool {
    args.grpc_endpoint.starts_with("https://")
        || args.grpc_tls_ca_path.is_some()
        || args.grpc_tls_cert_path.is_some()
        || args.grpc_tls_key_path.is_some()
        || args.grpc_tls_domain_name.is_some()
}

fn needs_nats_tls(args: &Args) -> bool {
    args.nats_url.starts_with("tls://")
        || args.nats_tls_ca_path.is_some()
        || args.nats_tls_cert_path.is_some()
        || args.nats_tls_key_path.is_some()
}

fn select_detail_node_id(
    args: &Args,
    fixture: &RelationRoundtripFixture,
    expected: &ExpectedProjection,
) -> Option<String> {
    args.detail_node_id.clone().or_else(|| {
        fixture.detail_node_id.clone().or_else(|| {
            expected
                .details
                .first()
                .map(|detail| detail.node_id.clone())
        })
    })
}

fn host_from_url(value: &str) -> Option<String> {
    Url::parse(value)
        .ok()
        .and_then(|url| url.host_str().map(ToString::to_string))
}

fn excerpt(value: &str, max_chars: usize) -> String {
    let total = value.chars().count();
    if total <= max_chars {
        return value.to_string();
    }
    let head: String = value.chars().take(max_chars).collect();
    format!("{head}...")
}

fn install_crypto_provider() {
    let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();
}

fn parse_args(
    mut args: impl Iterator<Item = String>,
) -> Result<Args, Box<dyn Error + Send + Sync>> {
    let mut input = None;
    let mut nats_url = None;
    let mut grpc_endpoint = None;
    let mut subject_prefix = "rehydration".to_string();
    let mut run_id = None;
    let mut role = "developer".to_string();
    let mut requested_scopes = vec!["graph".to_string(), "details".to_string()];
    let mut depth = 2_u32;
    let mut token_budget = 2048_u32;
    let mut rehydration_mode = 0_i32;
    let mut detail_node_id = None;
    let mut wait_timeout_secs = 20_u64;
    let mut poll_interval_ms = 250_u64;
    let mut grpc_tls_ca_path = None;
    let mut grpc_tls_cert_path = None;
    let mut grpc_tls_key_path = None;
    let mut grpc_tls_domain_name = None;
    let mut nats_tls_ca_path = None;
    let mut nats_tls_cert_path = None;
    let mut nats_tls_key_path = None;
    let mut nats_tls_first = false;
    let mut include_rendered_content = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => input = args.next(),
            "--nats-url" => nats_url = args.next(),
            "--grpc-endpoint" => grpc_endpoint = args.next(),
            "--subject-prefix" => {
                subject_prefix = required_flag_value(&mut args, "--subject-prefix")?
            }
            "--run-id" => run_id = Some(required_flag_value(&mut args, "--run-id")?),
            "--role" => role = required_flag_value(&mut args, "--role")?,
            "--requested-scope" => {
                if requested_scopes == ["graph".to_string(), "details".to_string()] {
                    requested_scopes.clear();
                }
                requested_scopes.push(required_flag_value(&mut args, "--requested-scope")?);
            }
            "--depth" => depth = parse_u32_flag(&mut args, "--depth")?,
            "--token-budget" => token_budget = parse_u32_flag(&mut args, "--token-budget")?,
            "--rehydration-mode" => {
                rehydration_mode = parse_rehydration_mode_flag(&mut args, "--rehydration-mode")?
            }
            "--detail-node-id" => {
                detail_node_id = Some(required_flag_value(&mut args, "--detail-node-id")?)
            }
            "--wait-timeout-secs" => {
                wait_timeout_secs = parse_u64_flag(&mut args, "--wait-timeout-secs")?
            }
            "--poll-interval-ms" => {
                poll_interval_ms = parse_u64_flag(&mut args, "--poll-interval-ms")?
            }
            "--grpc-tls-ca-path" => {
                grpc_tls_ca_path = Some(required_flag_value(&mut args, "--grpc-tls-ca-path")?)
            }
            "--grpc-tls-cert-path" => {
                grpc_tls_cert_path = Some(required_flag_value(&mut args, "--grpc-tls-cert-path")?)
            }
            "--grpc-tls-key-path" => {
                grpc_tls_key_path = Some(required_flag_value(&mut args, "--grpc-tls-key-path")?)
            }
            "--grpc-tls-domain-name" => {
                grpc_tls_domain_name =
                    Some(required_flag_value(&mut args, "--grpc-tls-domain-name")?)
            }
            "--nats-tls-ca-path" => {
                nats_tls_ca_path = Some(required_flag_value(&mut args, "--nats-tls-ca-path")?)
            }
            "--nats-tls-cert-path" => {
                nats_tls_cert_path = Some(required_flag_value(&mut args, "--nats-tls-cert-path")?)
            }
            "--nats-tls-key-path" => {
                nats_tls_key_path = Some(required_flag_value(&mut args, "--nats-tls-key-path")?)
            }
            "--nats-tls-first" => nats_tls_first = true,
            "--include-rendered-content" => include_rendered_content = true,
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
        input: required_value(input, "--input")?,
        nats_url: required_value(nats_url, "--nats-url")?,
        grpc_endpoint: required_value(grpc_endpoint, "--grpc-endpoint")?,
        subject_prefix,
        run_id: required_value(run_id, "--run-id")?,
        role,
        requested_scopes,
        depth,
        token_budget,
        rehydration_mode,
        detail_node_id,
        wait_timeout_secs,
        poll_interval_ms,
        grpc_tls_ca_path,
        grpc_tls_cert_path,
        grpc_tls_key_path,
        grpc_tls_domain_name,
        nats_tls_ca_path,
        nats_tls_cert_path,
        nats_tls_key_path,
        nats_tls_first,
        include_rendered_content,
    })
}

fn required_value(
    value: Option<String>,
    flag: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    value.ok_or_else(|| {
        Box::new(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{flag} is required"),
        )) as Box<dyn Error + Send + Sync>
    })
}

fn required_flag_value(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    required_value(args.next(), flag)
}

fn parse_u32_flag(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<u32, Box<dyn Error + Send + Sync>> {
    let value = required_flag_value(args, flag)?;
    value.parse::<u32>().map_err(|error| {
        Box::new(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid value for {flag}: {error}"),
        )) as Box<dyn Error + Send + Sync>
    })
}

fn parse_u64_flag(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<u64, Box<dyn Error + Send + Sync>> {
    let value = required_flag_value(args, flag)?;
    value.parse::<u64>().map_err(|error| {
        Box::new(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid value for {flag}: {error}"),
        )) as Box<dyn Error + Send + Sync>
    })
}

fn parse_rehydration_mode_flag(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<i32, Box<dyn Error + Send + Sync>> {
    match required_flag_value(args, flag)?.as_str() {
        "auto" | "unspecified" => Ok(0),
        "resume_focused" => Ok(1),
        "reason_preserving" => Ok(2),
        "temporal_delta" => Ok(3),
        "global_summary" => Ok(4),
        other => Err(Box::new(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid value for {flag}: {other}"),
        ))),
    }
}

fn print_usage() {
    eprintln!(
        "usage: graph_relation_roundtrip --input <path|-> --nats-url <url> --grpc-endpoint <url> --run-id <id> [--subject-prefix <prefix>] [--role <role>] [--requested-scope <scope>] [--depth <n>] [--token-budget <n>] [--rehydration-mode auto|resume_focused|reason_preserving] [--detail-node-id <node>] [--wait-timeout-secs <n>] [--poll-interval-ms <n>] [--grpc-tls-ca-path <path>] [--grpc-tls-cert-path <path>] [--grpc-tls-key-path <path>] [--grpc-tls-domain-name <name>] [--nats-tls-ca-path <path>] [--nats-tls-cert-path <path>] [--nats-tls-key-path <path>] [--nats-tls-first] [--include-rendered-content]"
    );
}

#[cfg(test)]
mod tests {
    use super::{
        ExpectedDetail, ExpectedProjection, ExpectedRelation, parse_args,
        projection_matches_expected, select_detail_node_id, substitute_run_id,
    };
    use kmp_proto::v1beta1::{
        BundleNodeDetail, GetContextResponse, GraphNode, GraphRelationship,
        GraphRelationshipExplanation, GraphRoleBundle, RehydrationBundle, RenderedContext,
    };

    const FIXTURE: &str = include_str!(
        "../../../../api/examples/kernel/v1beta1/async/pir-sequential-spine.relation-roundtrip.json"
    );

    #[test]
    fn substitute_run_id_rewrites_fixture_tokens() {
        let rewritten = substitute_run_id(FIXTURE, "run-123");
        assert!(rewritten.contains("incident:pir-run-123:cache-stampede"));
        assert!(!rewritten.contains("{{RUN_ID}}"));
    }

    #[test]
    fn parse_args_reads_required_and_optional_values() {
        let args = parse_args(
            [
                "--input",
                "fixture.json",
                "--nats-url",
                "tls://nats.example:4222",
                "--grpc-endpoint",
                "https://kernel.example.com",
                "--run-id",
                "run-123",
                "--requested-scope",
                "graph",
                "--requested-scope",
                "details",
                "--role",
                "incident-commander",
                "--depth",
                "3",
                "--token-budget",
                "4096",
                "--rehydration-mode",
                "reason_preserving",
                "--grpc-tls-domain-name",
                "kernel.example.com",
                "--nats-tls-first",
                "--include-rendered-content",
            ]
            .into_iter()
            .map(ToString::to_string),
        )
        .expect("args should parse");

        assert_eq!(args.input, "fixture.json");
        assert_eq!(args.run_id, "run-123");
        assert_eq!(args.role, "incident-commander");
        assert_eq!(args.rehydration_mode, 2);
        assert!(args.include_rendered_content);
        assert!(args.nats_tls_first);
    }

    #[test]
    fn select_detail_node_prefers_fixture_then_expected() {
        let fixture = serde_json::from_str::<super::RelationRoundtripFixture>(&substitute_run_id(
            FIXTURE, "run-123",
        ))
        .expect("fixture should parse");
        let expected = ExpectedProjection {
            node_ids: vec![
                "incident:pir-run-123:cache-stampede".to_string(),
                "finding:pir-run-123:cache-stampede:stampede".to_string(),
            ],
            relations: vec![],
            details: vec![ExpectedDetail {
                node_id: "finding:pir-run-123:cache-stampede:stampede".to_string(),
                detail: "detail".to_string(),
                revision: 1,
            }],
        };
        let args = parse_args(
            [
                "--input",
                "fixture.json",
                "--nats-url",
                "nats://localhost:4222",
                "--grpc-endpoint",
                "http://localhost:50054",
                "--run-id",
                "run-123",
            ]
            .into_iter()
            .map(ToString::to_string),
        )
        .expect("args should parse");

        assert_eq!(
            select_detail_node_id(&args, &fixture, &expected).as_deref(),
            Some("decision:pir-run-123:enable-jitter")
        );
    }

    #[test]
    fn projection_match_accepts_relation_only_spine_edge() {
        let fixture = serde_json::from_str::<super::RelationRoundtripFixture>(&substitute_run_id(
            FIXTURE, "run-123",
        ))
        .expect("fixture should parse");
        let expected = ExpectedProjection {
            node_ids: vec![
                "incident:pir-run-123:cache-stampede".to_string(),
                "finding:pir-run-123:cache-stampede:stampede".to_string(),
                "decision:pir-run-123:enable-jitter".to_string(),
            ],
            relations: vec![
                ExpectedRelation {
                    source_node_id: "incident:pir-run-123:cache-stampede".to_string(),
                    target_node_id: "finding:pir-run-123:cache-stampede:stampede".to_string(),
                    relation_type: "HAS_FINDING".to_string(),
                    semantic_class: Some(
                        kmp_proto::v1beta1::GraphRelationshipSemanticClass::Evidential
                            as i32,
                    ),
                },
                ExpectedRelation {
                    source_node_id: "incident:pir-run-123:cache-stampede".to_string(),
                    target_node_id: "decision:pir-run-123:enable-jitter".to_string(),
                    relation_type: "MITIGATED_BY".to_string(),
                    semantic_class: Some(
                        kmp_proto::v1beta1::GraphRelationshipSemanticClass::Procedural
                            as i32,
                    ),
                },
                ExpectedRelation {
                    source_node_id: "decision:pir-run-123:enable-jitter".to_string(),
                    target_node_id: "finding:pir-run-123:cache-stampede:stampede".to_string(),
                    relation_type: "ADDRESSES".to_string(),
                    semantic_class: Some(
                        kmp_proto::v1beta1::GraphRelationshipSemanticClass::Causal
                            as i32,
                    ),
                },
            ],
            details: vec![ExpectedDetail {
                node_id: "decision:pir-run-123:enable-jitter".to_string(),
                detail: "Apply a bounded jitter window to cache invalidation, then verify miss rate and checkout p95 before widening rollout.".to_string(),
                revision: 1,
            }],
        };

        let context = GetContextResponse {
            bundle: Some(RehydrationBundle {
                root_node_id: fixture.root_node_id.clone(),
                bundles: vec![GraphRoleBundle {
                    role: "developer".to_string(),
                    root_node: Some(GraphNode {
                        node_id: fixture.root_node_id.clone(),
                        node_kind: "incident".to_string(),
                        title: "Payments cache stampede".to_string(),
                        summary: "summary".to_string(),
                        status: "ACTIVE".to_string(),
                        labels: vec!["incident".to_string()],
                        properties: Default::default(),
                        provenance: None,
                    }),
                    neighbor_nodes: vec![
                        GraphNode {
                            node_id: "finding:pir-run-123:cache-stampede:stampede".to_string(),
                            node_kind: "finding".to_string(),
                            title: "Cache stampede on invalidation".to_string(),
                            summary: "summary".to_string(),
                            status: "ACTIVE".to_string(),
                            labels: vec!["finding".to_string()],
                            properties: Default::default(),
                            provenance: None,
                        },
                        GraphNode {
                            node_id: "decision:pir-run-123:enable-jitter".to_string(),
                            node_kind: "decision".to_string(),
                            title: "Enable cache jitter".to_string(),
                            summary: "summary".to_string(),
                            status: "PROPOSED".to_string(),
                            labels: vec!["decision".to_string()],
                            properties: Default::default(),
                            provenance: None,
                        },
                    ],
                    relationships: vec![
                        relationship(
                            "incident:pir-run-123:cache-stampede",
                            "finding:pir-run-123:cache-stampede:stampede",
                            "HAS_FINDING",
                            kmp_proto::v1beta1::GraphRelationshipSemanticClass::Evidential
                                as i32,
                        ),
                        relationship(
                            "incident:pir-run-123:cache-stampede",
                            "decision:pir-run-123:enable-jitter",
                            "MITIGATED_BY",
                            kmp_proto::v1beta1::GraphRelationshipSemanticClass::Procedural
                                as i32,
                        ),
                        relationship(
                            "decision:pir-run-123:enable-jitter",
                            "finding:pir-run-123:cache-stampede:stampede",
                            "ADDRESSES",
                            kmp_proto::v1beta1::GraphRelationshipSemanticClass::Causal
                                as i32,
                        ),
                    ],
                    node_details: vec![BundleNodeDetail {
                        node_id: "decision:pir-run-123:enable-jitter".to_string(),
                        detail: "Apply a bounded jitter window to cache invalidation, then verify miss rate and checkout p95 before widening rollout.".to_string(),
                        content_hash: "hash-1".to_string(),
                        revision: 1,
                    }],
                    rendered: None,
                }],
                ..Default::default()
            }),
            rendered: Some(RenderedContext {
                content: "Enable cache jitter".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };

        projection_matches_expected(&context, None, &fixture, &expected, None, true)
            .expect("projection should match expected relation-only shape");
    }

    fn relationship(
        source: &str,
        target: &str,
        relation_type: &str,
        semantic_class: i32,
    ) -> GraphRelationship {
        GraphRelationship {
            source_node_id: source.to_string(),
            target_node_id: target.to_string(),
            relationship_type: relation_type.to_string(),
            explanation: Some(GraphRelationshipExplanation {
                semantic_class,
                ..Default::default()
            }),
            provenance: None,
        }
    }
}
