use std::collections::HashMap;

use kmp_proto::v1beta1::{
    BundleNodeDetail, BundleVersion, GetContextResponse, GraphNode, GraphRelationship,
    GraphRelationshipExplanation, GraphRelationshipSemanticClass, GraphRoleBundle,
    RehydrateSessionResponse, RehydrationBundle, RehydrationStats, RenderedContext,
    ScopeValidationResult, UpdateContextResponse, ValidateScopeResponse,
};

use kmp_tests_shared::seed::kernel_data::{
    DECISION_DETAIL, DECISION_DETAIL_REVISION, DECISION_ID, DECISION_KIND, DECISION_LABEL,
    DECISION_STATUS, DECISION_SUMMARY, DECISION_TITLE, DEVELOPER_ROLE, HAS_TASK_RELATION,
    RECORDS_RELATION, ROOT_CREATED_BY, ROOT_DETAIL, ROOT_DETAIL_REVISION, ROOT_LABEL, ROOT_NODE_ID,
    ROOT_NODE_KIND, ROOT_PLAN_ID, ROOT_STATUS, ROOT_SUMMARY, ROOT_TITLE, TASK_ID, TASK_KIND,
    TASK_LABEL, TASK_PRIORITY, TASK_ROLE, TASK_STATUS, TASK_SUMMARY, TASK_TITLE,
};

pub(crate) fn expected_get_context_response() -> GetContextResponse {
    GetContextResponse {
        bundle: Some(expected_bundle()),
        rendered: Some(normalized_rendered_stub()),
        scope_validation: None,
        served_at: None,
        timing: None,
    }
}

pub(crate) fn expected_rehydrate_session_response(
    timeline_events: u32,
    snapshot_persisted: bool,
) -> RehydrateSessionResponse {
    RehydrateSessionResponse {
        bundle: Some(RehydrationBundle {
            root_node_id: ROOT_NODE_ID.to_string(),
            bundles: vec![expected_role_bundle()],
            stats: Some(RehydrationStats {
                roles: 1,
                nodes: 3,
                relationships: 2,
                detailed_nodes: 2,
                timeline_events,
            }),
            version: Some(expected_bundle_version("v1beta1")),
        }),
        snapshot_persisted,
        snapshot_id: if snapshot_persisted {
            format!("snapshot:{ROOT_NODE_ID}:{DEVELOPER_ROLE}")
        } else {
            String::new()
        },
        generated_at: None,
        timing: None,
    }
}

pub(crate) fn expected_validate_scope_allowed_response() -> ValidateScopeResponse {
    ValidateScopeResponse {
        result: Some(ScopeValidationResult {
            allowed: true,
            required_scopes: vec![
                "CASE_HEADER".to_string(),
                "DECISIONS_RELEVANT_ROLE".to_string(),
                "DEPS_RELEVANT".to_string(),
                "PLAN_HEADER".to_string(),
                "SUBTASKS_ROLE".to_string(),
            ],
            provided_scopes: vec![
                "CASE_HEADER".to_string(),
                "DECISIONS_RELEVANT_ROLE".to_string(),
                "DEPS_RELEVANT".to_string(),
                "PLAN_HEADER".to_string(),
                "SUBTASKS_ROLE".to_string(),
            ],
            missing_scopes: Vec::new(),
            extra_scopes: Vec::new(),
            reason: "scope validation passed".to_string(),
            diagnostics: Vec::new(),
        }),
    }
}

pub(crate) fn expected_validate_scope_rejected_response() -> ValidateScopeResponse {
    ValidateScopeResponse {
        result: Some(ScopeValidationResult {
            allowed: false,
            required_scopes: vec![
                "CASE_HEADER".to_string(),
                "DECISIONS_RELEVANT_ROLE".to_string(),
                "DEPS_RELEVANT".to_string(),
                "PLAN_HEADER".to_string(),
                "SUBTASKS_ROLE".to_string(),
            ],
            provided_scopes: vec![
                "CASE_HEADER".to_string(),
                "INVALID_SCOPE".to_string(),
                "PLAN_HEADER".to_string(),
                "SUBTASKS_ROLE".to_string(),
            ],
            missing_scopes: vec![
                "DECISIONS_RELEVANT_ROLE".to_string(),
                "DEPS_RELEVANT".to_string(),
            ],
            extra_scopes: vec!["INVALID_SCOPE".to_string()],
            reason: "scope validation failed".to_string(),
            diagnostics: Vec::new(),
        }),
    }
}

pub(crate) fn expected_update_context_response() -> UpdateContextResponse {
    UpdateContextResponse {
        accepted_version: Some(BundleVersion {
            revision: 1,
            content_hash: String::new(),
            schema_version: "v1beta1".to_string(),
            projection_watermark: "rev-1".to_string(),
            generated_at: None,
            generator_version: env!("CARGO_PKG_VERSION").to_string(),
        }),
        warnings: vec![],
    }
}

pub(crate) fn normalize_get_context_response(
    mut response: GetContextResponse,
) -> GetContextResponse {
    response.served_at = None;
    response.timing = None;
    response.rendered = response.rendered.map(normalize_rendered);
    if let Some(bundle) = response.bundle.as_mut() {
        normalize_bundle(bundle);
    }
    response
}

pub(crate) fn normalize_rehydrate_session_response(
    mut response: RehydrateSessionResponse,
) -> RehydrateSessionResponse {
    response.generated_at = None;
    response.timing = None;
    if let Some(bundle) = response.bundle.as_mut() {
        normalize_bundle(bundle);
    }
    response
}

pub(crate) fn normalize_update_context_response(
    mut response: UpdateContextResponse,
) -> UpdateContextResponse {
    if let Some(version) = response.accepted_version.as_mut() {
        version.generated_at = None;
        version.content_hash = String::new();
    }
    response
}

/// Strips rendered context to a normalized stub.
/// Rendering (salience ordering, cl100k tokens, tiers, modes) is covered
/// by the application-layer unit tests. The golden contract tests verify
/// the gRPC bundle structure, not the renderer output.
fn normalize_rendered(mut rendered: RenderedContext) -> RenderedContext {
    rendered.format = 0;
    rendered.token_count = 0;
    rendered.content = String::new();
    rendered.sections.clear();
    rendered.tiers.clear();
    rendered.resolved_mode = 0;
    rendered
}

fn normalized_rendered_stub() -> RenderedContext {
    normalize_rendered(RenderedContext::default())
}

fn normalize_bundle(bundle: &mut RehydrationBundle) {
    if let Some(version) = bundle.version.as_mut() {
        version.generated_at = None;
    }
}

fn expected_bundle() -> RehydrationBundle {
    RehydrationBundle {
        root_node_id: ROOT_NODE_ID.to_string(),
        bundles: vec![expected_role_bundle()],
        stats: Some(RehydrationStats {
            roles: 1,
            nodes: 3,
            relationships: 2,
            detailed_nodes: 2,
            timeline_events: 0,
        }),
        version: Some(expected_bundle_version("v1beta1")),
    }
}

fn expected_role_bundle() -> GraphRoleBundle {
    GraphRoleBundle {
        role: DEVELOPER_ROLE.to_string(),
        root_node: Some(expected_root_node()),
        neighbor_nodes: expected_neighbor_nodes(),
        relationships: expected_relationships(),
        node_details: vec![
            BundleNodeDetail {
                node_id: ROOT_NODE_ID.to_string(),
                detail: ROOT_DETAIL.to_string(),
                content_hash: "hash-story".to_string(),
                revision: ROOT_DETAIL_REVISION,
            },
            BundleNodeDetail {
                node_id: DECISION_ID.to_string(),
                detail: DECISION_DETAIL.to_string(),
                content_hash: "hash-decision".to_string(),
                revision: DECISION_DETAIL_REVISION,
            },
        ],
        rendered: None,
    }
}

fn expected_root_node() -> GraphNode {
    GraphNode {
        node_id: ROOT_NODE_ID.to_string(),
        node_kind: ROOT_NODE_KIND.to_string(),
        title: ROOT_TITLE.to_string(),
        summary: ROOT_SUMMARY.to_string(),
        status: ROOT_STATUS.to_string(),
        labels: vec![ROOT_LABEL.to_string()],
        properties: HashMap::from([
            ("created_by".to_string(), ROOT_CREATED_BY.to_string()),
            ("plan_id".to_string(), ROOT_PLAN_ID.to_string()),
        ]),
        provenance: None,
    }
}

fn expected_neighbor_nodes() -> Vec<GraphNode> {
    vec![
        GraphNode {
            node_id: DECISION_ID.to_string(),
            node_kind: DECISION_KIND.to_string(),
            title: DECISION_TITLE.to_string(),
            summary: DECISION_SUMMARY.to_string(),
            status: DECISION_STATUS.to_string(),
            labels: vec![DECISION_LABEL.to_string()],
            properties: HashMap::new(),
            provenance: None,
        },
        GraphNode {
            node_id: TASK_ID.to_string(),
            node_kind: TASK_KIND.to_string(),
            title: TASK_TITLE.to_string(),
            summary: TASK_SUMMARY.to_string(),
            status: TASK_STATUS.to_string(),
            labels: vec![TASK_LABEL.to_string()],
            properties: HashMap::from([
                ("priority".to_string(), TASK_PRIORITY.to_string()),
                ("role".to_string(), TASK_ROLE.to_string()),
            ]),
            provenance: None,
        },
    ]
}

fn expected_relationships() -> Vec<GraphRelationship> {
    vec![
        GraphRelationship {
            source_node_id: ROOT_NODE_ID.to_string(),
            target_node_id: DECISION_ID.to_string(),
            relationship_type: RECORDS_RELATION.to_string(),
            explanation: Some(GraphRelationshipExplanation {
                semantic_class: GraphRelationshipSemanticClass::Structural as i32,
                rationale: String::new(),
                motivation: String::new(),
                method: String::new(),
                decision_id: String::new(),
                caused_by_node_id: String::new(),
                evidence: String::new(),
                confidence: String::new(),
                sequence: 1,
                dimension: String::new(),
                scope_id: String::new(),
                occurred_at: String::new(),
                observed_at: String::new(),
                ingested_at: String::new(),
                valid_from: String::new(),
                valid_until: String::new(),
                rank: 0,
            }),
            provenance: None,
        },
        GraphRelationship {
            source_node_id: ROOT_NODE_ID.to_string(),
            target_node_id: TASK_ID.to_string(),
            relationship_type: HAS_TASK_RELATION.to_string(),
            explanation: Some(GraphRelationshipExplanation {
                semantic_class: GraphRelationshipSemanticClass::Motivational as i32,
                rationale: "the task operationalizes the selected beta kernel approach".to_string(),
                motivation: String::new(),
                method: String::new(),
                decision_id: String::new(),
                caused_by_node_id: String::new(),
                evidence: String::new(),
                confidence: String::new(),
                sequence: 2,
                dimension: String::new(),
                scope_id: String::new(),
                occurred_at: String::new(),
                observed_at: String::new(),
                ingested_at: String::new(),
                valid_from: String::new(),
                valid_until: String::new(),
                rank: 0,
            }),
            provenance: None,
        },
    ]
}

fn expected_bundle_version(schema_version: &str) -> BundleVersion {
    BundleVersion {
        revision: 1,
        content_hash: "pending".to_string(),
        schema_version: schema_version.to_string(),
        projection_watermark: "rev-1".to_string(),
        generated_at: None,
        generator_version: env!("CARGO_PKG_VERSION").to_string(),
    }
}
