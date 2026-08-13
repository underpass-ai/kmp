#![allow(dead_code)]

use std::error::Error;
use std::time::Instant;

use kmp_proto::v1beta1::{
    GetContextPathRequest, GraphRelationship, GraphRelationshipExplanation, GraphRoleBundle,
    RehydrationBundle, RenderedContext, context_query_service_client::ContextQueryServiceClient,
};
use tonic::transport::Channel;

use crate::fixtures::TestFixture;
use crate::metrics::{PaperMetricRelationship, PaperUseCaseMetric, ratio};
use crate::ports::ClosureSeed;
use crate::seed::explanatory_data::{
    ARTIFACT_NODE_ID, BAD_DECISION_NODE_ID, BAD_TASK_NODE_ID, CONSTRAINT_DECISION_NODE_ID,
    CONSTRAINT_RATIONALE, CONSTRAINT_RELATION, CONSTRAINT_ROOT_NODE_ID, CONSTRAINT_TASK_DETAIL,
    CONSTRAINT_TASK_MOTIVATION, CONSTRAINT_TASK_NODE_ID, CONSTRAINT_TASK_RATIONALE,
    DECISION_NODE_ID, DECISION_TO_TASK_MOTIVATION, DECISION_TO_TASK_RATIONALE,
    DECISION_TO_TASK_RELATION, DetailMode, FAILURE_DETAIL, FAILURE_EVIDENCE_RATIONALE,
    FAILURE_FOCUS_NODE_ID, FOCUS_DETAIL, FOCUS_NODE_ID, GraphScale, HANDOFF_BLOCKER_NODE_ID,
    HANDOFF_BLOCKER_RATIONALE, HANDOFF_RESUME_DECISION_NODE_ID, HANDOFF_RESUME_MOTIVATION,
    HANDOFF_RESUME_RATIONALE, HANDOFF_RESUMED_DETAIL, HANDOFF_RESUMED_TASK_NODE_ID,
    HANDOFF_ROOT_NODE_ID, HANDOFF_SUCCESS_ARTIFACT_NODE_ID, HANDOFF_SUCCESS_RATIONALE,
    HANDOFF_TASK_STARTED_NODE_ID, ProjectionSeedVariant, RECOVERY_DECISION_RATIONALE,
    RECOVERY_DECISION_RELATION, RECOVERY_SUCCESS_RATIONALE, ROOT_NODE_ID, RelationExplanationMode,
    TASK_TO_ARTIFACT_METHOD, TASK_TO_ARTIFACT_RELATION,
    publish_constraint_projection_events_variant, publish_explanatory_projection_events_variant,
    publish_flawed_task_projection_events_variant, publish_handoff_projection_events_variant,
};

pub const DEFAULT_TOKEN_BUDGET: u32 = 4096;
pub const LOW_TOKEN_BUDGET: u32 = 192;
pub const STRUCTURAL_LOW_TOKEN_BUDGET: u32 = 96;

pub const FAILURE_DIAGNOSIS_USE_CASE_ID: &str = "uc1_failure_diagnosis_rehydration";
pub const FAILURE_DIAGNOSIS_TITLE: &str = "Failure diagnosis and rehydration-point recovery";
pub const FAILURE_DIAGNOSIS_QUESTION: &str = "Given a graph that implemented a task incorrectly, can we isolate the suspect relationships and identify the upstream rehydration point?";
pub const WHY_IMPLEMENTED_USE_CASE_ID: &str = "uc2_why_implementation_trace";
pub const WHY_IMPLEMENTED_TITLE: &str = "Why-was-this-implemented-like-this analysis";
pub const WHY_IMPLEMENTED_QUESTION: &str = "Given a task that was implemented in a specific way, can we reconstruct the reason for that implementation choice from graph context alone?";
pub const HANDOFF_RESUME_USE_CASE_ID: &str = "uc3_interrupted_handoff_resume";
pub const HANDOFF_RESUME_TITLE: &str = "Interrupted handoff and resumable execution";
pub const HANDOFF_RESUME_QUESTION: &str = "Given a task interrupted after a failed implementation step, can the system recover why it was handed off and from which upstream node execution should resume?";
pub const CONSTRAINT_PRESSURE_USE_CASE_ID: &str = "uc4_constraint_reason_under_token_pressure";
pub const CONSTRAINT_PRESSURE_TITLE: &str = "Constraint-preserving retrieval under token pressure";
pub const CONSTRAINT_PRESSURE_QUESTION: &str = "Given a task chosen under a binding safety constraint, can the system preserve the dominant reason even when the rendered context is strongly budgeted?";

#[derive(Debug, Clone, Copy)]
pub struct PaperUseCaseVariant {
    pub variant_id: &'static str,
    pub variant_label: &'static str,
    pub seed_variant: ProjectionSeedVariant,
}

impl PaperUseCaseVariant {
    pub const FULL_EXPLANATORY_WITH_DETAIL: Self = Self {
        variant_id: "full_explanatory_with_detail",
        variant_label: "Full explanatory relations with detail",
        seed_variant: ProjectionSeedVariant::FULL_EXPLANATORY_WITH_DETAIL,
    };

    pub const STRUCTURAL_ONLY_WITH_DETAIL: Self = Self {
        variant_id: "structural_only_with_detail",
        variant_label: "Structural-only relations with detail",
        seed_variant: ProjectionSeedVariant::STRUCTURAL_ONLY_WITH_DETAIL,
    };

    pub const DETAIL_ONLY_WITH_DETAIL: Self = Self {
        variant_id: "detail_only_with_detail",
        variant_label: "Detail-only baseline with structural relations",
        seed_variant: ProjectionSeedVariant::DETAIL_ONLY_WITH_DETAIL,
    };

    pub const FULL_EXPLANATORY_WITHOUT_DETAIL: Self = Self {
        variant_id: "full_explanatory_without_detail",
        variant_label: "Full explanatory relations without detail",
        seed_variant: ProjectionSeedVariant::FULL_EXPLANATORY_WITHOUT_DETAIL,
    };

    pub fn relation_variant_label(self) -> &'static str {
        match self.seed_variant.relation_mode {
            RelationExplanationMode::Explanatory => "explanatory",
            RelationExplanationMode::StructuralOnly => "structural_only",
            RelationExplanationMode::DetailOnly => "detail_only",
        }
    }

    pub fn detail_variant_label(self) -> &'static str {
        match self.seed_variant.detail_mode {
            DetailMode::WithDetail => "with_detail",
            DetailMode::WithoutDetail => "without_detail",
        }
    }

    pub fn graph_scale_label(self) -> &'static str {
        match self.seed_variant.graph_scale {
            GraphScale::Micro => "micro",
            GraphScale::Meso => "meso",
        }
    }

    pub fn with_graph_scale(self, graph_scale: GraphScale) -> Self {
        Self {
            seed_variant: self.seed_variant.with_graph_scale(graph_scale),
            ..self
        }
    }

    pub fn metric_variant_id(self, token_budget: u32) -> String {
        let mut variant_id = self.variant_id.to_string();
        if self.seed_variant.graph_scale == GraphScale::Meso {
            variant_id.push_str("__meso");
        }
        if token_budget != DEFAULT_TOKEN_BUDGET {
            variant_id.push_str(&format!("__budget_{token_budget}"));
        }
        variant_id
    }

    pub fn metric_variant_label(self, token_budget: u32) -> String {
        let mut label = self.variant_label.to_string();
        if self.seed_variant.graph_scale == GraphScale::Meso {
            label.push_str(" (meso)");
        }
        if token_budget != DEFAULT_TOKEN_BUDGET {
            label.push_str(&format!(" @ {token_budget} tokens"));
        }
        label
    }
}

pub struct FailureDiagnosisObservation {
    pub metric: PaperUseCaseMetric,
    pub suspect_relationships: Vec<PaperMetricRelationship>,
    pub rehydration_node_id: String,
    pub rendered_content: String,
}

pub struct ImplementationWhyObservation {
    pub metric: PaperUseCaseMetric,
    pub rationale: String,
    pub motivation: String,
    pub decision_id: String,
    pub caused_by_node_id: String,
    pub rendered_content: String,
}

pub struct HandoffResumeObservation {
    pub metric: PaperUseCaseMetric,
    pub continuation_node_id: String,
    pub blocker_rationale: String,
    pub motivation: String,
    pub rendered_content: String,
}

pub struct ConstraintPressureObservation {
    pub metric: PaperUseCaseMetric,
    pub constraint_rationale: String,
    pub implementation_rationale: String,
    pub rendered_content: String,
}

struct RetryPathObservation {
    bundle: RehydrationBundle,
    rendered: RenderedContext,
}

#[derive(Debug, Clone)]
struct RelationshipExplanationView {
    rationale: String,
    motivation: String,
    method: String,
    decision_id: String,
    caused_by_node_id: String,
    evidence: String,
    confidence: String,
    sequence: u32,
}

pub async fn observe_failure_diagnosis_use_case(
    variant: PaperUseCaseVariant,
) -> Result<FailureDiagnosisObservation, Box<dyn Error + Send + Sync>> {
    let seed_variant = variant.seed_variant;
    let fixture = TestFixture::builder()
        .with_neo4j()
        .with_valkey()
        .with_nats()
        .with_projection_runtime()
        .with_grpc_server()
        .with_seed(ClosureSeed::new(move |ctx| {
            let client = ctx.nats_client().clone();
            Box::pin(async move {
                publish_flawed_task_projection_events_variant(&client, seed_variant).await
            })
        }))
        .with_readiness_check(ROOT_NODE_ID, FAILURE_FOCUS_NODE_ID)
        .require_node_detail(seed_variant.detail_mode == DetailMode::WithDetail)
        .build()
        .await?;

    let result: Result<FailureDiagnosisObservation, Box<dyn Error + Send + Sync>> = async {
        let total_start = Instant::now();
        let mut query_client = fixture.query_client();
        let query_start = Instant::now();
        let path = query_client
            .get_context_path(GetContextPathRequest {
                root_node_id: ROOT_NODE_ID.to_string(),
                target_node_id: FAILURE_FOCUS_NODE_ID.to_string(),
                role: "implementer".to_string(),
                token_budget: DEFAULT_TOKEN_BUDGET,
            })
            .await?
            .into_inner();
        let query_latency_ms = query_start.elapsed().as_secs_f64() * 1000.0;
        let kmp_proto::v1beta1::GetContextPathResponse {
            path_bundle,
            rendered,
            timing,
            ..
        } = path;
        let (graph_load_ms, detail_load_ms, bundle_assembly_ms, detail_batch_size) =
            if let Some(ref timing) = timing {
                (
                    Some(timing.graph_load_seconds * 1000.0),
                    Some(timing.detail_load_seconds * 1000.0),
                    Some(timing.bundle_assembly_seconds * 1000.0),
                    Some(timing.batch_size),
                )
            } else {
                (None, None, None, None)
            };

        let bundle = expect_path_bundle(path_bundle)?;
        let role_bundle = first_role_bundle(&bundle)?;
        let failure_edge = find_relationship(
            &role_bundle.relationships,
            BAD_TASK_NODE_ID,
            FAILURE_FOCUS_NODE_ID,
            TASK_TO_ARTIFACT_RELATION,
        )?;
        let failure_explanation = relationship_explanation(failure_edge)?;
        let rehydration_node_id = failure_explanation.caused_by_node_id.clone();
        let suspect_relationships = if rehydration_node_id.is_empty() {
            Vec::new()
        } else {
            role_bundle
                .relationships
                .iter()
                .filter(|relationship| {
                    relationship_explanation(relationship)
                        .map(|explanation| {
                            explanation.decision_id == rehydration_node_id
                                || explanation.caused_by_node_id == rehydration_node_id
                        })
                        .unwrap_or(false)
                })
                .map(metric_relationship)
                .collect::<Vec<_>>()
        };
        let retry_path = load_retry_path(
            &mut query_client,
            &rehydration_node_id,
            ARTIFACT_NODE_ID,
            DEFAULT_TOKEN_BUDGET,
        )
        .await?;
        let (retry_success_hit, retry_success_rate, retry_target_node_id) =
            if let Some(retry_path) = retry_path {
                let retry_role_bundle = first_role_bundle(&retry_path.bundle)?;
                let retry_rendered_contains_expected_rationale = retry_path
                    .rendered
                    .content
                    .contains(RECOVERY_DECISION_RATIONALE)
                    && retry_path
                        .rendered
                        .content
                        .contains(RECOVERY_SUCCESS_RATIONALE);
                let retry_rendered_contains_expected_decision_reference = retry_path
                    .rendered
                    .content
                    .contains(&format!("decision={DECISION_NODE_ID}"));
                let retry_contains_recovery_decision = contains_relationship(
                    &retry_role_bundle.relationships,
                    BAD_DECISION_NODE_ID,
                    DECISION_NODE_ID,
                    RECOVERY_DECISION_RELATION,
                );
                let retry_contains_correct_task = contains_relationship(
                    &retry_role_bundle.relationships,
                    DECISION_NODE_ID,
                    FOCUS_NODE_ID,
                    DECISION_TO_TASK_RELATION,
                );
                let retry_contains_success_verification = contains_relationship(
                    &retry_role_bundle.relationships,
                    FOCUS_NODE_ID,
                    ARTIFACT_NODE_ID,
                    TASK_TO_ARTIFACT_RELATION,
                );
                let retry_success_rate = ratio(
                    [
                        rehydration_node_id == BAD_DECISION_NODE_ID,
                        retry_contains_recovery_decision,
                        retry_contains_correct_task,
                        retry_contains_success_verification,
                        retry_rendered_contains_expected_rationale,
                        retry_rendered_contains_expected_decision_reference,
                    ]
                    .into_iter()
                    .filter(|hit| *hit)
                    .count(),
                    6,
                );
                let retry_success_hit = retry_contains_recovery_decision
                    && retry_contains_correct_task
                    && retry_contains_success_verification
                    && retry_rendered_contains_expected_rationale
                    && retry_rendered_contains_expected_decision_reference;

                (
                    Some(retry_success_hit),
                    Some(retry_success_rate),
                    Some(ARTIFACT_NODE_ID.to_string()),
                )
            } else {
                (Some(false), Some(0.0), Some(ARTIFACT_NODE_ID.to_string()))
            };
        let rendered = expect_rendered_context(rendered)?;
        let rendered_contains_expected_rationale =
            rendered.content.contains(FAILURE_EVIDENCE_RATIONALE);
        let rendered_contains_expected_decision_reference = rendered
            .content
            .contains(&format!("decision={BAD_DECISION_NODE_ID}"));
        let rendered_contains_expected_detail = rendered.content.contains(FAILURE_DETAIL);
        let explanation_roundtrip_fidelity = ratio(
            [
                failure_explanation.rationale == FAILURE_EVIDENCE_RATIONALE,
                failure_explanation.decision_id == BAD_DECISION_NODE_ID,
                failure_explanation.caused_by_node_id == BAD_DECISION_NODE_ID,
                failure_explanation.method == TASK_TO_ARTIFACT_METHOD,
            ]
            .into_iter()
            .filter(|hit| *hit)
            .count(),
            4,
        );
        let detail_roundtrip_fidelity = if rendered_contains_expected_detail {
            1.0
        } else {
            0.0
        };
        let causal_reconstruction_score = ratio(
            [
                rehydration_node_id == BAD_DECISION_NODE_ID,
                contains_metric_relationship(
                    &suspect_relationships,
                    BAD_DECISION_NODE_ID,
                    BAD_TASK_NODE_ID,
                    DECISION_TO_TASK_RELATION,
                ),
                contains_metric_relationship(
                    &suspect_relationships,
                    BAD_TASK_NODE_ID,
                    FAILURE_FOCUS_NODE_ID,
                    TASK_TO_ARTIFACT_RELATION,
                ),
                suspect_relationships.len() == 2,
                rendered_contains_expected_rationale,
                rendered_contains_expected_decision_reference,
                rendered_contains_expected_detail,
            ]
            .into_iter()
            .filter(|hit| *hit)
            .count(),
            7,
        );

        let total_latency_ms = total_start.elapsed().as_secs_f64() * 1000.0;

        let llm_eval = maybe_evaluate_with_llm(
            &rendered.content,
            FAILURE_DIAGNOSIS_QUESTION,
            Some("the decision to preserve comfort load, caused by the port manifold breach"),
            Some("the decision to preserve comfort load — the point where the causal chain can be corrected"),
            Some(RECOVERY_DECISION_RATIONALE),
        )
        .await;

        Ok(FailureDiagnosisObservation {
            metric: PaperUseCaseMetric {
                use_case_id: FAILURE_DIAGNOSIS_USE_CASE_ID.to_string(),
                variant_id: variant.metric_variant_id(DEFAULT_TOKEN_BUDGET),
                variant_label: variant.metric_variant_label(DEFAULT_TOKEN_BUDGET),
                relation_variant: variant.relation_variant_label().to_string(),
                detail_variant: variant.detail_variant_label().to_string(),
                graph_scale: variant.graph_scale_label().to_string(),
                requested_token_budget: DEFAULT_TOKEN_BUDGET,
                title: FAILURE_DIAGNOSIS_TITLE.to_string(),
                question: FAILURE_DIAGNOSIS_QUESTION.to_string(),
                root_node_id: ROOT_NODE_ID.to_string(),
                target_node_id: FAILURE_FOCUS_NODE_ID.to_string(),
                bundle_nodes: bundle_node_count(&bundle),
                bundle_relationships: role_bundle.relationships.len() as u32,
                detailed_nodes: role_bundle.node_details.len() as u32,
                rendered_token_count: rendered.token_count,
                query_latency_ms: Some(query_latency_ms),
                total_latency_ms: Some(total_latency_ms),
                graph_load_ms,
                detail_load_ms,
                bundle_assembly_ms,
                detail_batch_size,
                explanation_roundtrip_fidelity,
                detail_roundtrip_fidelity,
                causal_reconstruction_score,
                rendered_contains_expected_rationale,
                rendered_contains_expected_decision_reference,
                rendered_contains_expected_detail,
                rehydration_point_hit: Some(rehydration_node_id == BAD_DECISION_NODE_ID),
                rehydration_node_id: proto_string(&rehydration_node_id),
                retry_success_hit,
                retry_success_rate,
                retry_target_node_id,
                dominant_reason_hit: None,
                suspect_relationship_count: Some(suspect_relationships.len() as u32),
                full_graph_relationship_count: None,
                rationale: proto_string(&failure_explanation.rationale),
                motivation: proto_string(&failure_explanation.motivation),
                method: proto_string(&failure_explanation.method),
                decision_id: proto_string(&failure_explanation.decision_id),
                caused_by_node_id: proto_string(&failure_explanation.caused_by_node_id),
                suspect_relationships: suspect_relationships.clone(),
                llm_task_success: llm_eval.as_ref().map(|e| e.llm_task_success),
                llm_restart_accuracy: llm_eval.as_ref().map(|e| e.llm_restart_accuracy),
                llm_restart_exact: llm_eval.as_ref().map(|e| e.llm_restart_exact),
                llm_restart_off_by_one: llm_eval.as_ref().map(|e| e.llm_restart_off_by_one),
                llm_restart_on_competing: llm_eval.as_ref().map(|e| e.llm_restart_on_competing),
                llm_restart_explained: llm_eval.as_ref().map(|e| e.llm_restart_explained),
                llm_reason_preserved: llm_eval.as_ref().map(|e| e.llm_reason_preserved),
                llm_reason_correct: llm_eval.as_ref().map(|e| e.llm_reason_correct),
                llm_reason_distractor: llm_eval.as_ref().map(|e| e.llm_reason_distractor),
                llm_latency_ms: llm_eval.as_ref().map(|e| e.llm_latency_ms),
                llm_judge_raw: llm_eval.as_ref().and_then(|e| e.llm_judge_raw.clone()),
            },
            suspect_relationships,
            rehydration_node_id,
            rendered_content: rendered.content,
        })
    }
    .await;

    let shutdown_result = fixture.shutdown().await;
    let observation = result?;
    shutdown_result?;
    Ok(observation)
}

pub async fn observe_why_task_was_implemented_that_way(
    variant: PaperUseCaseVariant,
) -> Result<ImplementationWhyObservation, Box<dyn Error + Send + Sync>> {
    let seed_variant = variant.seed_variant;
    let fixture = TestFixture::builder()
        .with_neo4j()
        .with_valkey()
        .with_nats()
        .with_projection_runtime()
        .with_grpc_server()
        .with_seed(ClosureSeed::new(move |ctx| {
            let client = ctx.nats_client().clone();
            Box::pin(async move {
                publish_explanatory_projection_events_variant(&client, seed_variant).await
            })
        }))
        .with_readiness_check(ROOT_NODE_ID, FOCUS_NODE_ID)
        .require_node_detail(seed_variant.detail_mode == DetailMode::WithDetail)
        .build()
        .await?;

    let result: Result<ImplementationWhyObservation, Box<dyn Error + Send + Sync>> = async {
        let total_start = Instant::now();
        let mut query_client = fixture.query_client();
        let query_start = Instant::now();
        let path = query_client
            .get_context_path(GetContextPathRequest {
                root_node_id: ROOT_NODE_ID.to_string(),
                target_node_id: FOCUS_NODE_ID.to_string(),
                role: "implementer".to_string(),
                token_budget: DEFAULT_TOKEN_BUDGET,
            })
            .await?
            .into_inner();
        let query_latency_ms = query_start.elapsed().as_secs_f64() * 1000.0;
        let kmp_proto::v1beta1::GetContextPathResponse {
            path_bundle,
            rendered,
            timing,
            ..
        } = path;
        let (graph_load_ms, detail_load_ms, bundle_assembly_ms, detail_batch_size) =
            if let Some(ref timing) = timing {
                (
                    Some(timing.graph_load_seconds * 1000.0),
                    Some(timing.detail_load_seconds * 1000.0),
                    Some(timing.bundle_assembly_seconds * 1000.0),
                    Some(timing.batch_size),
                )
            } else {
                (None, None, None, None)
            };

        let bundle = expect_path_bundle(path_bundle)?;
        let role_bundle = first_role_bundle(&bundle)?;
        let decision_edge = find_relationship(
            &role_bundle.relationships,
            DECISION_NODE_ID,
            FOCUS_NODE_ID,
            DECISION_TO_TASK_RELATION,
        )?;
        let explanation = relationship_explanation(decision_edge)?;
        let rendered = expect_rendered_context(rendered)?;
        let rendered_contains_expected_rationale =
            rendered.content.contains(DECISION_TO_TASK_RATIONALE);
        let rendered_contains_expected_decision_reference = rendered
            .content
            .contains(&format!("decision={DECISION_NODE_ID}"));
        let rendered_contains_expected_detail = rendered.content.contains(FOCUS_DETAIL);
        let explanation_roundtrip_fidelity = ratio(
            [
                explanation.rationale == DECISION_TO_TASK_RATIONALE,
                explanation.motivation == DECISION_TO_TASK_MOTIVATION,
                explanation.decision_id == DECISION_NODE_ID,
                explanation.caused_by_node_id == ROOT_NODE_ID,
            ]
            .into_iter()
            .filter(|hit| *hit)
            .count(),
            4,
        );
        let detail_roundtrip_fidelity = if rendered_contains_expected_detail {
            1.0
        } else {
            0.0
        };
        let causal_reconstruction_score = ratio(
            [
                explanation.rationale == DECISION_TO_TASK_RATIONALE,
                explanation.motivation == DECISION_TO_TASK_MOTIVATION,
                explanation.decision_id == DECISION_NODE_ID,
                explanation.caused_by_node_id == ROOT_NODE_ID,
                rendered_contains_expected_rationale,
                rendered_contains_expected_decision_reference,
                rendered_contains_expected_detail,
            ]
            .into_iter()
            .filter(|hit| *hit)
            .count(),
            7,
        );

        let total_latency_ms = total_start.elapsed().as_secs_f64() * 1000.0;

        let llm_eval = maybe_evaluate_with_llm(
            &rendered.content,
            WHY_IMPLEMENTED_QUESTION,
            None,
            None,
            Some(DECISION_TO_TASK_RATIONALE),
        )
        .await;

        Ok(ImplementationWhyObservation {
            metric: PaperUseCaseMetric {
                use_case_id: WHY_IMPLEMENTED_USE_CASE_ID.to_string(),
                variant_id: variant.metric_variant_id(DEFAULT_TOKEN_BUDGET),
                variant_label: variant.metric_variant_label(DEFAULT_TOKEN_BUDGET),
                relation_variant: variant.relation_variant_label().to_string(),
                detail_variant: variant.detail_variant_label().to_string(),
                graph_scale: variant.graph_scale_label().to_string(),
                requested_token_budget: DEFAULT_TOKEN_BUDGET,
                title: WHY_IMPLEMENTED_TITLE.to_string(),
                question: WHY_IMPLEMENTED_QUESTION.to_string(),
                root_node_id: ROOT_NODE_ID.to_string(),
                target_node_id: FOCUS_NODE_ID.to_string(),
                bundle_nodes: bundle_node_count(&bundle),
                bundle_relationships: role_bundle.relationships.len() as u32,
                detailed_nodes: role_bundle.node_details.len() as u32,
                rendered_token_count: rendered.token_count,
                query_latency_ms: Some(query_latency_ms),
                total_latency_ms: Some(total_latency_ms),
                graph_load_ms,
                detail_load_ms,
                bundle_assembly_ms,
                detail_batch_size,
                explanation_roundtrip_fidelity,
                detail_roundtrip_fidelity,
                causal_reconstruction_score,
                rendered_contains_expected_rationale,
                rendered_contains_expected_decision_reference,
                rendered_contains_expected_detail,
                rehydration_point_hit: None,
                rehydration_node_id: None,
                retry_success_hit: None,
                retry_success_rate: None,
                retry_target_node_id: None,
                dominant_reason_hit: None,
                suspect_relationship_count: None,
                full_graph_relationship_count: None,
                rationale: proto_string(&explanation.rationale),
                motivation: proto_string(&explanation.motivation),
                method: proto_string(&explanation.method),
                decision_id: proto_string(&explanation.decision_id),
                caused_by_node_id: proto_string(&explanation.caused_by_node_id),
                suspect_relationships: Vec::new(),
                llm_task_success: llm_eval.as_ref().map(|e| e.llm_task_success),
                llm_restart_accuracy: llm_eval.as_ref().map(|e| e.llm_restart_accuracy),
                llm_restart_exact: llm_eval.as_ref().map(|e| e.llm_restart_exact),
                llm_restart_off_by_one: llm_eval.as_ref().map(|e| e.llm_restart_off_by_one),
                llm_restart_on_competing: llm_eval.as_ref().map(|e| e.llm_restart_on_competing),
                llm_restart_explained: llm_eval.as_ref().map(|e| e.llm_restart_explained),
                llm_reason_preserved: llm_eval.as_ref().map(|e| e.llm_reason_preserved),
                llm_reason_correct: llm_eval.as_ref().map(|e| e.llm_reason_correct),
                llm_reason_distractor: llm_eval.as_ref().map(|e| e.llm_reason_distractor),
                llm_latency_ms: llm_eval.as_ref().map(|e| e.llm_latency_ms),
                llm_judge_raw: llm_eval.as_ref().and_then(|e| e.llm_judge_raw.clone()),
            },
            rationale: explanation.rationale.clone(),
            motivation: explanation.motivation.clone(),
            decision_id: explanation.decision_id.clone(),
            caused_by_node_id: explanation.caused_by_node_id.clone(),
            rendered_content: rendered.content,
        })
    }
    .await;

    let shutdown_result = fixture.shutdown().await;
    let observation = result?;
    shutdown_result?;
    Ok(observation)
}

pub async fn observe_interrupted_handoff_use_case(
    variant: PaperUseCaseVariant,
) -> Result<HandoffResumeObservation, Box<dyn Error + Send + Sync>> {
    let seed_variant = variant.seed_variant;
    let fixture = TestFixture::builder()
        .with_neo4j()
        .with_valkey()
        .with_nats()
        .with_projection_runtime()
        .with_grpc_server()
        .with_seed(ClosureSeed::new(move |ctx| {
            let client = ctx.nats_client().clone();
            Box::pin(async move {
                publish_handoff_projection_events_variant(&client, seed_variant).await
            })
        }))
        .with_readiness_check(HANDOFF_ROOT_NODE_ID, HANDOFF_RESUMED_TASK_NODE_ID)
        .require_node_detail(seed_variant.detail_mode == DetailMode::WithDetail)
        .build()
        .await?;

    let result: Result<HandoffResumeObservation, Box<dyn Error + Send + Sync>> = async {
        let total_start = Instant::now();
        let mut query_client = fixture.query_client();
        let query_start = Instant::now();
        let path = query_client
            .get_context_path(GetContextPathRequest {
                root_node_id: HANDOFF_ROOT_NODE_ID.to_string(),
                target_node_id: HANDOFF_RESUMED_TASK_NODE_ID.to_string(),
                role: "implementer".to_string(),
                token_budget: DEFAULT_TOKEN_BUDGET,
            })
            .await?
            .into_inner();
        let query_latency_ms = query_start.elapsed().as_secs_f64() * 1000.0;
        let kmp_proto::v1beta1::GetContextPathResponse {
            path_bundle,
            rendered,
            timing,
            ..
        } = path;
        let (graph_load_ms, detail_load_ms, bundle_assembly_ms, detail_batch_size) =
            if let Some(ref timing) = timing {
                (
                    Some(timing.graph_load_seconds * 1000.0),
                    Some(timing.detail_load_seconds * 1000.0),
                    Some(timing.bundle_assembly_seconds * 1000.0),
                    Some(timing.batch_size),
                )
            } else {
                (None, None, None, None)
            };

        let bundle = expect_path_bundle(path_bundle)?;
        let role_bundle = first_role_bundle(&bundle)?;
        let blocker_edge = find_relationship(
            &role_bundle.relationships,
            HANDOFF_TASK_STARTED_NODE_ID,
            HANDOFF_BLOCKER_NODE_ID,
            "BLOCKED_BY",
        )?;
        let blocker_explanation = relationship_explanation(blocker_edge)?;
        let resume_edge = find_relationship(
            &role_bundle.relationships,
            HANDOFF_RESUME_DECISION_NODE_ID,
            HANDOFF_RESUMED_TASK_NODE_ID,
            DECISION_TO_TASK_RELATION,
        )?;
        let resume_explanation = relationship_explanation(resume_edge)?;
        let continuation_node_id = resume_explanation.caused_by_node_id.clone();
        let retry_path = load_retry_path(
            &mut query_client,
            &continuation_node_id,
            HANDOFF_SUCCESS_ARTIFACT_NODE_ID,
            DEFAULT_TOKEN_BUDGET,
        )
        .await?;
        let (retry_success_hit, retry_success_rate, retry_target_node_id) =
            if let Some(retry_path) = retry_path {
                let retry_role_bundle = first_role_bundle(&retry_path.bundle)?;
                let retry_rendered_contains_expected_rationale = retry_path
                    .rendered
                    .content
                    .contains(HANDOFF_RESUME_RATIONALE)
                    && retry_path
                        .rendered
                        .content
                        .contains(HANDOFF_SUCCESS_RATIONALE);
                let retry_rendered_contains_expected_decision_reference = retry_path
                    .rendered
                    .content
                    .contains(&format!("decision={HANDOFF_RESUME_DECISION_NODE_ID}"));
                let retry_contains_blocker = contains_relationship(
                    &retry_role_bundle.relationships,
                    HANDOFF_TASK_STARTED_NODE_ID,
                    HANDOFF_BLOCKER_NODE_ID,
                    "BLOCKED_BY",
                );
                let retry_contains_resume_edge = contains_relationship(
                    &retry_role_bundle.relationships,
                    HANDOFF_RESUME_DECISION_NODE_ID,
                    HANDOFF_RESUMED_TASK_NODE_ID,
                    DECISION_TO_TASK_RELATION,
                );
                let retry_contains_success_verification = contains_relationship(
                    &retry_role_bundle.relationships,
                    HANDOFF_RESUMED_TASK_NODE_ID,
                    HANDOFF_SUCCESS_ARTIFACT_NODE_ID,
                    TASK_TO_ARTIFACT_RELATION,
                );
                let retry_success_rate = ratio(
                    [
                        continuation_node_id == HANDOFF_TASK_STARTED_NODE_ID,
                        retry_contains_blocker,
                        retry_contains_resume_edge,
                        retry_contains_success_verification,
                        retry_rendered_contains_expected_rationale,
                        retry_rendered_contains_expected_decision_reference,
                    ]
                    .into_iter()
                    .filter(|hit| *hit)
                    .count(),
                    6,
                );
                let retry_success_hit = retry_contains_blocker
                    && retry_contains_resume_edge
                    && retry_contains_success_verification
                    && retry_rendered_contains_expected_rationale
                    && retry_rendered_contains_expected_decision_reference;

                (
                    Some(retry_success_hit),
                    Some(retry_success_rate),
                    Some(HANDOFF_SUCCESS_ARTIFACT_NODE_ID.to_string()),
                )
            } else {
                (
                    Some(false),
                    Some(0.0),
                    Some(HANDOFF_SUCCESS_ARTIFACT_NODE_ID.to_string()),
                )
            };
        let rendered = expect_rendered_context(rendered)?;
        let rendered_contains_expected_rationale =
            rendered.content.contains(HANDOFF_RESUME_RATIONALE)
                && rendered.content.contains(HANDOFF_BLOCKER_RATIONALE);
        let rendered_contains_expected_decision_reference = rendered
            .content
            .contains(&format!("decision={HANDOFF_RESUME_DECISION_NODE_ID}"));
        let rendered_contains_expected_detail = rendered.content.contains(HANDOFF_RESUMED_DETAIL);
        let explanation_roundtrip_fidelity = ratio(
            [
                blocker_explanation.rationale == HANDOFF_BLOCKER_RATIONALE,
                resume_explanation.rationale == HANDOFF_RESUME_RATIONALE,
                resume_explanation.motivation == HANDOFF_RESUME_MOTIVATION,
                resume_explanation.decision_id == HANDOFF_RESUME_DECISION_NODE_ID,
                resume_explanation.caused_by_node_id == HANDOFF_TASK_STARTED_NODE_ID,
            ]
            .into_iter()
            .filter(|hit| *hit)
            .count(),
            5,
        );
        let detail_roundtrip_fidelity = if rendered_contains_expected_detail {
            1.0
        } else {
            0.0
        };
        let causal_reconstruction_score = ratio(
            [
                continuation_node_id == HANDOFF_TASK_STARTED_NODE_ID,
                blocker_explanation.rationale == HANDOFF_BLOCKER_RATIONALE,
                resume_explanation.rationale == HANDOFF_RESUME_RATIONALE,
                resume_explanation.motivation == HANDOFF_RESUME_MOTIVATION,
                rendered_contains_expected_rationale,
                rendered_contains_expected_decision_reference,
                rendered_contains_expected_detail,
            ]
            .into_iter()
            .filter(|hit| *hit)
            .count(),
            7,
        );

        let total_latency_ms = total_start.elapsed().as_secs_f64() * 1000.0;

        let llm_eval = maybe_evaluate_with_llm(
            &rendered.content,
            HANDOFF_RESUME_QUESTION,
            Some("the manual override jam that blocked the remote isolation attempt"),
            Some("the point where remote isolation was blocked — resume with manual override"),
            Some(HANDOFF_BLOCKER_RATIONALE),
        )
        .await;

        Ok(HandoffResumeObservation {
            metric: PaperUseCaseMetric {
                use_case_id: HANDOFF_RESUME_USE_CASE_ID.to_string(),
                variant_id: variant.metric_variant_id(DEFAULT_TOKEN_BUDGET),
                variant_label: variant.metric_variant_label(DEFAULT_TOKEN_BUDGET),
                relation_variant: variant.relation_variant_label().to_string(),
                detail_variant: variant.detail_variant_label().to_string(),
                graph_scale: variant.graph_scale_label().to_string(),
                requested_token_budget: DEFAULT_TOKEN_BUDGET,
                title: HANDOFF_RESUME_TITLE.to_string(),
                question: HANDOFF_RESUME_QUESTION.to_string(),
                root_node_id: HANDOFF_ROOT_NODE_ID.to_string(),
                target_node_id: HANDOFF_RESUMED_TASK_NODE_ID.to_string(),
                bundle_nodes: bundle_node_count(&bundle),
                bundle_relationships: role_bundle.relationships.len() as u32,
                detailed_nodes: role_bundle.node_details.len() as u32,
                rendered_token_count: rendered.token_count,
                query_latency_ms: Some(query_latency_ms),
                total_latency_ms: Some(total_latency_ms),
                graph_load_ms,
                detail_load_ms,
                bundle_assembly_ms,
                detail_batch_size,
                explanation_roundtrip_fidelity,
                detail_roundtrip_fidelity,
                causal_reconstruction_score,
                rendered_contains_expected_rationale,
                rendered_contains_expected_decision_reference,
                rendered_contains_expected_detail,
                rehydration_point_hit: Some(continuation_node_id == HANDOFF_TASK_STARTED_NODE_ID),
                rehydration_node_id: proto_string(&continuation_node_id),
                retry_success_hit,
                retry_success_rate,
                retry_target_node_id,
                dominant_reason_hit: None,
                suspect_relationship_count: None,
                full_graph_relationship_count: None,
                rationale: proto_string(&resume_explanation.rationale),
                motivation: proto_string(&resume_explanation.motivation),
                method: proto_string(&blocker_explanation.method),
                decision_id: proto_string(&resume_explanation.decision_id),
                caused_by_node_id: proto_string(&resume_explanation.caused_by_node_id),
                suspect_relationships: Vec::new(),
                llm_task_success: llm_eval.as_ref().map(|e| e.llm_task_success),
                llm_restart_accuracy: llm_eval.as_ref().map(|e| e.llm_restart_accuracy),
                llm_restart_exact: llm_eval.as_ref().map(|e| e.llm_restart_exact),
                llm_restart_off_by_one: llm_eval.as_ref().map(|e| e.llm_restart_off_by_one),
                llm_restart_on_competing: llm_eval.as_ref().map(|e| e.llm_restart_on_competing),
                llm_restart_explained: llm_eval.as_ref().map(|e| e.llm_restart_explained),
                llm_reason_preserved: llm_eval.as_ref().map(|e| e.llm_reason_preserved),
                llm_reason_correct: llm_eval.as_ref().map(|e| e.llm_reason_correct),
                llm_reason_distractor: llm_eval.as_ref().map(|e| e.llm_reason_distractor),
                llm_latency_ms: llm_eval.as_ref().map(|e| e.llm_latency_ms),
                llm_judge_raw: llm_eval.as_ref().and_then(|e| e.llm_judge_raw.clone()),
            },
            continuation_node_id,
            blocker_rationale: blocker_explanation.rationale.clone(),
            motivation: resume_explanation.motivation.clone(),
            rendered_content: rendered.content,
        })
    }
    .await;

    let shutdown_result = fixture.shutdown().await;
    let observation = result?;
    shutdown_result?;
    Ok(observation)
}

pub async fn observe_constraint_under_token_pressure_use_case(
    variant: PaperUseCaseVariant,
    token_budget: u32,
) -> Result<ConstraintPressureObservation, Box<dyn Error + Send + Sync>> {
    let seed_variant = variant.seed_variant;
    let fixture = TestFixture::builder()
        .with_neo4j()
        .with_valkey()
        .with_nats()
        .with_projection_runtime()
        .with_grpc_server()
        .with_seed(ClosureSeed::new(move |ctx| {
            let client = ctx.nats_client().clone();
            Box::pin(async move {
                publish_constraint_projection_events_variant(&client, seed_variant).await
            })
        }))
        .with_readiness_check(CONSTRAINT_ROOT_NODE_ID, CONSTRAINT_TASK_NODE_ID)
        .require_node_detail(seed_variant.detail_mode == DetailMode::WithDetail)
        .build()
        .await?;

    let result: Result<ConstraintPressureObservation, Box<dyn Error + Send + Sync>> = async {
        let total_start = Instant::now();
        let mut query_client = fixture.query_client();
        let query_start = Instant::now();
        let path = query_client
            .get_context_path(GetContextPathRequest {
                root_node_id: CONSTRAINT_ROOT_NODE_ID.to_string(),
                target_node_id: CONSTRAINT_TASK_NODE_ID.to_string(),
                role: "implementer".to_string(),
                token_budget,
            })
            .await?
            .into_inner();
        let query_latency_ms = query_start.elapsed().as_secs_f64() * 1000.0;
        let kmp_proto::v1beta1::GetContextPathResponse {
            path_bundle,
            rendered,
            timing,
            ..
        } = path;
        let (graph_load_ms, detail_load_ms, bundle_assembly_ms, detail_batch_size) =
            if let Some(ref timing) = timing {
                (
                    Some(timing.graph_load_seconds * 1000.0),
                    Some(timing.detail_load_seconds * 1000.0),
                    Some(timing.bundle_assembly_seconds * 1000.0),
                    Some(timing.batch_size),
                )
            } else {
                (None, None, None, None)
            };

        let bundle = expect_path_bundle(path_bundle)?;
        let role_bundle = first_role_bundle(&bundle)?;
        let constraint_edge = find_relationship(
            &role_bundle.relationships,
            CONSTRAINT_ROOT_NODE_ID,
            CONSTRAINT_DECISION_NODE_ID,
            CONSTRAINT_RELATION,
        )?;
        let constraint_explanation = relationship_explanation(constraint_edge)?;
        let implementation_edge = find_relationship(
            &role_bundle.relationships,
            CONSTRAINT_DECISION_NODE_ID,
            CONSTRAINT_TASK_NODE_ID,
            DECISION_TO_TASK_RELATION,
        )?;
        let implementation_explanation = relationship_explanation(implementation_edge)?;
        let rendered = expect_rendered_context(rendered)?;
        let rendered_contains_expected_rationale = rendered.content.contains(CONSTRAINT_RATIONALE);
        let rendered_contains_expected_decision_reference = rendered
            .content
            .contains(&format!("decision={CONSTRAINT_DECISION_NODE_ID}"));
        let rendered_contains_expected_detail = rendered.content.contains(CONSTRAINT_TASK_DETAIL);
        let explanation_roundtrip_fidelity = ratio(
            [
                constraint_explanation.rationale == CONSTRAINT_RATIONALE,
                implementation_explanation.rationale == CONSTRAINT_TASK_RATIONALE,
                implementation_explanation.motivation == CONSTRAINT_TASK_MOTIVATION,
                implementation_explanation.decision_id == CONSTRAINT_DECISION_NODE_ID,
                implementation_explanation.caused_by_node_id == CONSTRAINT_ROOT_NODE_ID,
            ]
            .into_iter()
            .filter(|hit| *hit)
            .count(),
            5,
        );
        let detail_roundtrip_fidelity = if rendered_contains_expected_detail {
            1.0
        } else {
            0.0
        };
        let dominant_reason_hit = rendered_contains_expected_rationale
            && implementation_explanation.caused_by_node_id == CONSTRAINT_ROOT_NODE_ID;
        let causal_reconstruction_score = ratio(
            [
                constraint_explanation.rationale == CONSTRAINT_RATIONALE,
                implementation_explanation.rationale == CONSTRAINT_TASK_RATIONALE,
                implementation_explanation.motivation == CONSTRAINT_TASK_MOTIVATION,
                implementation_explanation.decision_id == CONSTRAINT_DECISION_NODE_ID,
                rendered_contains_expected_rationale,
                rendered_contains_expected_decision_reference,
                dominant_reason_hit,
                rendered.token_count <= token_budget,
            ]
            .into_iter()
            .filter(|hit| *hit)
            .count(),
            8,
        );

        let total_latency_ms = total_start.elapsed().as_secs_f64() * 1000.0;

        let llm_eval = maybe_evaluate_with_llm(
            &rendered.content,
            CONSTRAINT_PRESSURE_QUESTION,
            None,
            None,
            Some(CONSTRAINT_RATIONALE),
        )
        .await;

        Ok(ConstraintPressureObservation {
            metric: PaperUseCaseMetric {
                use_case_id: CONSTRAINT_PRESSURE_USE_CASE_ID.to_string(),
                variant_id: variant.metric_variant_id(token_budget),
                variant_label: variant.metric_variant_label(token_budget),
                relation_variant: variant.relation_variant_label().to_string(),
                detail_variant: variant.detail_variant_label().to_string(),
                graph_scale: variant.graph_scale_label().to_string(),
                requested_token_budget: token_budget,
                title: CONSTRAINT_PRESSURE_TITLE.to_string(),
                question: CONSTRAINT_PRESSURE_QUESTION.to_string(),
                root_node_id: CONSTRAINT_ROOT_NODE_ID.to_string(),
                target_node_id: CONSTRAINT_TASK_NODE_ID.to_string(),
                bundle_nodes: bundle_node_count(&bundle),
                bundle_relationships: role_bundle.relationships.len() as u32,
                detailed_nodes: role_bundle.node_details.len() as u32,
                rendered_token_count: rendered.token_count,
                query_latency_ms: Some(query_latency_ms),
                total_latency_ms: Some(total_latency_ms),
                graph_load_ms,
                detail_load_ms,
                bundle_assembly_ms,
                detail_batch_size,
                explanation_roundtrip_fidelity,
                detail_roundtrip_fidelity,
                causal_reconstruction_score,
                rendered_contains_expected_rationale,
                rendered_contains_expected_decision_reference,
                rendered_contains_expected_detail,
                rehydration_point_hit: None,
                rehydration_node_id: None,
                retry_success_hit: None,
                retry_success_rate: None,
                retry_target_node_id: None,
                dominant_reason_hit: Some(dominant_reason_hit),
                suspect_relationship_count: None,
                full_graph_relationship_count: None,
                rationale: proto_string(&constraint_explanation.rationale),
                motivation: proto_string(&implementation_explanation.motivation),
                method: proto_string(&constraint_explanation.method),
                decision_id: proto_string(&implementation_explanation.decision_id),
                caused_by_node_id: proto_string(&implementation_explanation.caused_by_node_id),
                suspect_relationships: Vec::new(),
                llm_task_success: llm_eval.as_ref().map(|e| e.llm_task_success),
                llm_restart_accuracy: llm_eval.as_ref().map(|e| e.llm_restart_accuracy),
                llm_restart_exact: llm_eval.as_ref().map(|e| e.llm_restart_exact),
                llm_restart_off_by_one: llm_eval.as_ref().map(|e| e.llm_restart_off_by_one),
                llm_restart_on_competing: llm_eval.as_ref().map(|e| e.llm_restart_on_competing),
                llm_restart_explained: llm_eval.as_ref().map(|e| e.llm_restart_explained),
                llm_reason_preserved: llm_eval.as_ref().map(|e| e.llm_reason_preserved),
                llm_reason_correct: llm_eval.as_ref().map(|e| e.llm_reason_correct),
                llm_reason_distractor: llm_eval.as_ref().map(|e| e.llm_reason_distractor),
                llm_latency_ms: llm_eval.as_ref().map(|e| e.llm_latency_ms),
                llm_judge_raw: llm_eval.as_ref().and_then(|e| e.llm_judge_raw.clone()),
            },
            constraint_rationale: constraint_explanation.rationale.clone(),
            implementation_rationale: implementation_explanation.rationale.clone(),
            rendered_content: rendered.content,
        })
    }
    .await;

    let shutdown_result = fixture.shutdown().await;
    let observation = result?;
    shutdown_result?;
    Ok(observation)
}

async fn load_retry_path(
    query_client: &mut ContextQueryServiceClient<Channel>,
    root_node_id: &str,
    target_node_id: &str,
    token_budget: u32,
) -> Result<Option<RetryPathObservation>, Box<dyn Error + Send + Sync>> {
    if root_node_id.is_empty() {
        return Ok(None);
    }

    let path = query_client
        .get_context_path(GetContextPathRequest {
            root_node_id: root_node_id.to_string(),
            target_node_id: target_node_id.to_string(),
            role: "implementer".to_string(),
            token_budget,
        })
        .await?
        .into_inner();
    let kmp_proto::v1beta1::GetContextPathResponse {
        path_bundle,
        rendered,
        ..
    } = path;

    Ok(Some(RetryPathObservation {
        bundle: expect_path_bundle(path_bundle)?,
        rendered: expect_rendered_context(rendered)?,
    }))
}

fn contains_relationship(
    relationships: &[GraphRelationship],
    source_node_id: &str,
    target_node_id: &str,
    relationship_type: &str,
) -> bool {
    relationships.iter().any(|relationship| {
        relationship.source_node_id == source_node_id
            && relationship.target_node_id == target_node_id
            && relationship.relationship_type == relationship_type
    })
}

pub fn contains_metric_relationship(
    relationships: &[PaperMetricRelationship],
    source_node_id: &str,
    target_node_id: &str,
    relationship_type: &str,
) -> bool {
    relationships.iter().any(|relationship| {
        relationship.source_node_id == source_node_id
            && relationship.target_node_id == target_node_id
            && relationship.relationship_type == relationship_type
    })
}

fn expect_path_bundle(
    bundle: Option<RehydrationBundle>,
) -> Result<RehydrationBundle, Box<dyn Error + Send + Sync>> {
    bundle.ok_or_else(|| "path bundle should be present".to_string().into())
}

fn first_role_bundle(
    bundle: &RehydrationBundle,
) -> Result<&GraphRoleBundle, Box<dyn Error + Send + Sync>> {
    bundle.bundles.first().ok_or_else(|| {
        "path bundle should contain one role bundle"
            .to_string()
            .into()
    })
}

fn expect_rendered_context(
    rendered: Option<RenderedContext>,
) -> Result<RenderedContext, Box<dyn Error + Send + Sync>> {
    rendered.ok_or_else(|| "rendered context should be present".to_string().into())
}

fn bundle_node_count(bundle: &RehydrationBundle) -> u32 {
    bundle
        .stats
        .as_ref()
        .map(|stats| stats.nodes)
        .unwrap_or_else(|| {
            bundle
                .bundles
                .first()
                .map(|role_bundle| role_bundle.neighbor_nodes.len() as u32 + 1)
                .unwrap_or(0)
        })
}

fn find_relationship<'a>(
    relationships: &'a [GraphRelationship],
    source_node_id: &str,
    target_node_id: &str,
    relationship_type: &str,
) -> Result<&'a GraphRelationship, Box<dyn Error + Send + Sync>> {
    relationships
        .iter()
        .find(|relationship| {
            relationship.source_node_id == source_node_id
                && relationship.target_node_id == target_node_id
                && relationship.relationship_type == relationship_type
        })
        .ok_or_else(|| {
            format!(
                "relationship `{source_node_id}` --{relationship_type}--> `{target_node_id}` should exist"
            )
            .into()
        })
}

fn relationship_explanation(
    relationship: &GraphRelationship,
) -> Result<RelationshipExplanationView, Box<dyn Error + Send + Sync>> {
    let explanation = relationship
        .explanation
        .as_ref()
        .ok_or_else(|| "relationship explanation should be present".to_string())?;

    Ok(RelationshipExplanationView {
        rationale: proto_string_field(explanation, |it| &it.rationale),
        motivation: proto_string_field(explanation, |it| &it.motivation),
        method: proto_string_field(explanation, |it| &it.method),
        decision_id: proto_string_field(explanation, |it| &it.decision_id),
        caused_by_node_id: proto_string_field(explanation, |it| &it.caused_by_node_id),
        evidence: proto_string_field(explanation, |it| &it.evidence),
        confidence: proto_string_field(explanation, |it| &it.confidence),
        sequence: explanation.sequence,
    })
}

fn metric_relationship(relationship: &GraphRelationship) -> PaperMetricRelationship {
    PaperMetricRelationship {
        source_node_id: relationship.source_node_id.clone(),
        target_node_id: relationship.target_node_id.clone(),
        relationship_type: relationship.relationship_type.clone(),
    }
}

fn proto_string_field(
    explanation: &GraphRelationshipExplanation,
    selector: impl Fn(&GraphRelationshipExplanation) -> &str,
) -> String {
    selector(explanation).to_string()
}

fn proto_string(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

/// Optionally evaluate rendered context with an LLM if LLM_ENDPOINT is configured.
/// Returns None if LLM evaluation is not configured or fails.
pub async fn maybe_evaluate_with_llm(
    rendered_content: &str,
    question: &str,
    expected_failure_point: Option<&str>,
    expected_restart_node: Option<&str>,
    expected_reason: Option<&str>,
) -> Option<kmp_testkit::LlmEvaluationResult> {
    if std::env::var("LLM_ENDPOINT").is_err() {
        return None;
    }

    let config = kmp_testkit::LlmEvaluatorConfig::from_env();
    let ground_truth = kmp_testkit::EvaluationGroundTruth {
        expected_failure_point: expected_failure_point.map(str::to_string),
        expected_restart_node: expected_restart_node.map(str::to_string),
        expected_reason: expected_reason.map(str::to_string),
        distractor_rationale: None,
        domain_context: None,
    };

    match kmp_testkit::evaluate_with_llm(&config, rendered_content, question, &ground_truth).await {
        Ok(result) => Some(result),
        Err(error) => {
            eprintln!("LLM evaluation failed (non-fatal): {error}");
            None
        }
    }
}
