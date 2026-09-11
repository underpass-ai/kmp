use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_domain::*;

pub const ABOUT: &str = "project:evidence-paths";
pub const EARLY: &str = "2026-09-01T10:00:00Z";
pub const LATE: &str = "2026-09-02T10:00:00Z";
pub fn node(id: &str) -> ProjectionMutation {
    ProjectionMutation::UpsertNode(NodeProjection {
        node_id: id.into(),
        node_kind: "observation".into(),
        title: id.into(),
        summary: id.into(),
        status: "ACTIVE".into(),
        labels: vec!["entry".into()],
        properties: [("memory_about".into(), ABOUT.into())].into(),
        provenance: None,
    })
}
pub fn edge(a: &str, b: &str, relation: &str, at: Option<&str>) -> ProjectionMutation {
    let mut explanation = RelationExplanation::new(RelationSemanticClass::Evidential)
        .with_rationale(format!("Source explicitly links {a} to {b}."))
        .with_evidence(format!("Record: {a} {relation} {b}."));
    if let Some(at) = at {
        explanation = explanation.with_observed_at(at);
    }
    ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
        source_node_id: a.into(),
        target_node_id: b.into(),
        relation_type: relation.into(),
        explanation,
    }))
}
pub fn label(id: &str, key: &str, value: &str, at: &str) -> ProjectionMutation {
    let reference = MemoryDimensionIdentity::new(ABOUT, key, value)
        .expect("valid isolated fixture")
        .node_id();
    ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
        source_node_id: reference.clone(),
        target_node_id: id.into(),
        relation_type: "contains_entry".into(),
        explanation: RelationExplanation::new(RelationSemanticClass::Structural)
            .with_dimension(key)
            .with_scope_id(reference)
            .with_observed_at(at),
    }))
}
pub fn step(relation: &str) -> TraceRelationStep {
    TraceRelationStep {
        relation: MemoryRelationType::new(relation).expect("valid isolated fixture"),
        direction: RelationDirection::Outgoing,
    }
}
pub fn binding(at: u32) -> EvidencePathBinding {
    EvidencePathBinding::Label {
        at,
        name: "event".into(),
        key: "event".into(),
    }
}
pub fn role(
    name: &str,
    relations: &[&str],
    bindings: Vec<EvidencePathBinding>,
) -> EvidencePathRole {
    EvidencePathRole {
        name: name.into(),
        context: false,
        steps: relations.iter().map(|r| step(r)).collect(),
        bindings,
    }
}
pub fn request(roles: Vec<EvidencePathRole>) -> EvidencePathRequest {
    EvidencePathRequest {
        proof: false,
        about: ABOUT.into(),
        from: "s".into(),
        roles,
        constants: Default::default(),
        temporal: Default::default(),
        limits: TraceSearchLimits {
            nodes: 4096,
            edges: 32768,
            depth: 1024,
            states: 32768,
        },
    }
}
pub async fn store(mutations: Vec<ProjectionMutation>) -> (tempfile::TempDir, EmbeddedKernelStore) {
    let dir = tempfile::tempdir().expect("valid isolated fixture");
    let store = EmbeddedKernelStore::open(dir.path()).expect("valid isolated fixture");
    store
        .apply_mutations(mutations)
        .await
        .expect("valid isolated fixture");
    (dir, store)
}
