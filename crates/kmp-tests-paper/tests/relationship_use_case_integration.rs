#![cfg(feature = "container-tests")]

use std::error::Error;

use kmp_tests_shared::metrics::emit_metric;
use kmp_tests_shared::metrics::use_case_harness::{
    DEFAULT_TOKEN_BUDGET, LOW_TOKEN_BUDGET, PaperUseCaseVariant, contains_metric_relationship,
    observe_constraint_under_token_pressure_use_case, observe_failure_diagnosis_use_case,
    observe_interrupted_handoff_use_case, observe_why_task_was_implemented_that_way,
};
use kmp_tests_shared::seed::explanatory_data::{
    ARTIFACT_NODE_ID, BAD_DECISION_NODE_ID, BAD_DECISION_TO_TASK_RATIONALE, BAD_TASK_NODE_ID,
    CONSTRAINT_DECISION_NODE_ID, CONSTRAINT_RATIONALE, CONSTRAINT_TASK_DETAIL,
    CONSTRAINT_TASK_RATIONALE, DECISION_NODE_ID, DECISION_TO_TASK_MOTIVATION,
    DECISION_TO_TASK_RATIONALE, DECISION_TO_TASK_RELATION, FAILURE_DETAIL, FAILURE_FOCUS_NODE_ID,
    FOCUS_DETAIL, GraphScale, HANDOFF_BLOCKER_RATIONALE, HANDOFF_RESUME_DECISION_NODE_ID,
    HANDOFF_RESUME_MOTIVATION, HANDOFF_RESUME_RATIONALE, HANDOFF_RESUMED_DETAIL,
    HANDOFF_SUCCESS_ARTIFACT_NODE_ID, HANDOFF_TASK_STARTED_NODE_ID, TASK_TO_ARTIFACT_RELATION,
};

#[tokio::test]
async fn failing_graph_identifies_suspect_relationships_and_rehydration_point()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let observation =
        observe_failure_diagnosis_use_case(PaperUseCaseVariant::FULL_EXPLANATORY_WITH_DETAIL)
            .await?;

    assert_eq!(observation.rehydration_node_id, BAD_DECISION_NODE_ID);
    assert_eq!(observation.suspect_relationships.len(), 2);
    assert!(contains_metric_relationship(
        &observation.suspect_relationships,
        BAD_DECISION_NODE_ID,
        BAD_TASK_NODE_ID,
        DECISION_TO_TASK_RELATION,
    ));
    assert!(contains_metric_relationship(
        &observation.suspect_relationships,
        BAD_TASK_NODE_ID,
        FAILURE_FOCUS_NODE_ID,
        TASK_TO_ARTIFACT_RELATION,
    ));
    assert!(observation.rendered_content.contains(FAILURE_DETAIL));
    assert!(
        observation
            .rendered_content
            .contains(&format!("decision={BAD_DECISION_NODE_ID}"))
    );
    assert_eq!(observation.metric.explanation_roundtrip_fidelity, 1.0);
    assert_eq!(observation.metric.detail_roundtrip_fidelity, 1.0);
    assert_eq!(observation.metric.causal_reconstruction_score, 1.0);
    assert_eq!(observation.metric.retry_success_hit, Some(true));
    assert_eq!(observation.metric.retry_success_rate, Some(1.0));
    assert_eq!(
        observation.metric.retry_target_node_id.as_deref(),
        Some(ARTIFACT_NODE_ID)
    );

    emit_metric(&observation.metric)?;
    Ok(())
}

#[tokio::test]
async fn context_path_explains_why_a_task_was_implemented_that_way()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let observation = observe_why_task_was_implemented_that_way(
        PaperUseCaseVariant::FULL_EXPLANATORY_WITH_DETAIL,
    )
    .await?;

    assert_eq!(observation.rationale, DECISION_TO_TASK_RATIONALE);
    assert_eq!(observation.motivation, DECISION_TO_TASK_MOTIVATION);
    assert_eq!(observation.decision_id, DECISION_NODE_ID);
    assert_eq!(
        observation.caused_by_node_id,
        kmp_tests_shared::seed::explanatory_data::ROOT_NODE_ID
    );

    let why = format!("{} {}", observation.rationale, observation.motivation);
    assert!(why.contains(DECISION_TO_TASK_RATIONALE));
    assert!(why.contains(DECISION_TO_TASK_MOTIVATION));
    assert_ne!(why, BAD_DECISION_TO_TASK_RATIONALE);
    assert!(
        observation
            .rendered_content
            .contains(DECISION_TO_TASK_RATIONALE)
    );
    assert!(
        observation
            .rendered_content
            .contains(&format!("decision={DECISION_NODE_ID}"))
    );
    assert!(observation.rendered_content.contains(FOCUS_DETAIL));
    assert_eq!(observation.metric.explanation_roundtrip_fidelity, 1.0);
    assert_eq!(observation.metric.detail_roundtrip_fidelity, 1.0);
    assert_eq!(observation.metric.causal_reconstruction_score, 1.0);

    emit_metric(&observation.metric)?;
    Ok(())
}

#[tokio::test]
async fn interrupted_handoff_recovers_resume_anchor_and_reasoning()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let observation =
        observe_interrupted_handoff_use_case(PaperUseCaseVariant::FULL_EXPLANATORY_WITH_DETAIL)
            .await?;

    assert_eq!(
        observation.continuation_node_id,
        HANDOFF_TASK_STARTED_NODE_ID
    );
    assert_eq!(observation.blocker_rationale, HANDOFF_BLOCKER_RATIONALE);
    assert_eq!(observation.motivation, HANDOFF_RESUME_MOTIVATION);
    assert!(
        observation
            .rendered_content
            .contains(HANDOFF_RESUME_RATIONALE)
    );
    assert!(
        observation
            .rendered_content
            .contains(&format!("decision={HANDOFF_RESUME_DECISION_NODE_ID}"))
    );
    assert!(
        observation
            .rendered_content
            .contains(HANDOFF_RESUMED_DETAIL)
    );
    assert_eq!(observation.metric.explanation_roundtrip_fidelity, 1.0);
    assert_eq!(observation.metric.detail_roundtrip_fidelity, 1.0);
    assert_eq!(observation.metric.causal_reconstruction_score, 1.0);
    assert_eq!(observation.metric.retry_success_hit, Some(true));
    assert_eq!(observation.metric.retry_success_rate, Some(1.0));
    assert_eq!(
        observation.metric.retry_target_node_id.as_deref(),
        Some(HANDOFF_SUCCESS_ARTIFACT_NODE_ID)
    );

    emit_metric(&observation.metric)?;
    Ok(())
}

#[tokio::test]
async fn low_budget_context_preserves_constraint_reason_with_explanatory_relations()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let baseline = observe_constraint_under_token_pressure_use_case(
        PaperUseCaseVariant::FULL_EXPLANATORY_WITH_DETAIL,
        DEFAULT_TOKEN_BUDGET,
    )
    .await?;
    let low_budget = observe_constraint_under_token_pressure_use_case(
        PaperUseCaseVariant::FULL_EXPLANATORY_WITH_DETAIL,
        LOW_TOKEN_BUDGET,
    )
    .await?;

    assert_eq!(baseline.constraint_rationale, CONSTRAINT_RATIONALE);
    assert_eq!(baseline.implementation_rationale, CONSTRAINT_TASK_RATIONALE);
    assert!(baseline.rendered_content.contains(CONSTRAINT_TASK_DETAIL));
    assert_eq!(baseline.metric.explanation_roundtrip_fidelity, 1.0);
    assert_eq!(baseline.metric.detail_roundtrip_fidelity, 1.0);
    assert_eq!(baseline.metric.causal_reconstruction_score, 1.0);

    assert_eq!(low_budget.constraint_rationale, CONSTRAINT_RATIONALE);
    assert_eq!(
        low_budget.implementation_rationale,
        CONSTRAINT_TASK_RATIONALE
    );
    assert!(low_budget.rendered_content.contains(CONSTRAINT_RATIONALE));
    assert!(
        low_budget
            .rendered_content
            .contains(&format!("decision={CONSTRAINT_DECISION_NODE_ID}"))
    );
    assert!(!low_budget.rendered_content.contains(CONSTRAINT_TASK_DETAIL));
    assert_eq!(low_budget.metric.explanation_roundtrip_fidelity, 1.0);
    assert_eq!(low_budget.metric.detail_roundtrip_fidelity, 0.0);
    assert_eq!(low_budget.metric.dominant_reason_hit, Some(true));
    assert!(low_budget.metric.rendered_token_count <= LOW_TOKEN_BUDGET);
    assert_eq!(low_budget.metric.causal_reconstruction_score, 1.0);

    emit_metric(&baseline.metric)?;
    emit_metric(&low_budget.metric)?;
    Ok(())
}

#[tokio::test]
async fn detail_only_baseline_preserves_partial_why_trace_in_node_detail()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let structural_only =
        observe_why_task_was_implemented_that_way(PaperUseCaseVariant::STRUCTURAL_ONLY_WITH_DETAIL)
            .await?;
    let detail_only =
        observe_why_task_was_implemented_that_way(PaperUseCaseVariant::DETAIL_ONLY_WITH_DETAIL)
            .await?;

    assert_eq!(detail_only.metric.explanation_roundtrip_fidelity, 0.0);
    assert_eq!(detail_only.metric.detail_roundtrip_fidelity, 1.0);
    assert!(
        detail_only
            .rendered_content
            .contains(DECISION_TO_TASK_RATIONALE)
    );
    assert!(
        detail_only
            .rendered_content
            .contains(&format!("decision={DECISION_NODE_ID}"))
    );
    assert!(
        detail_only.metric.causal_reconstruction_score
            > structural_only.metric.causal_reconstruction_score
    );

    emit_metric(&detail_only.metric)?;
    Ok(())
}

#[tokio::test]
async fn meso_failure_diagnosis_retains_rehydration_signal_under_noise()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let observation = observe_failure_diagnosis_use_case(
        PaperUseCaseVariant::FULL_EXPLANATORY_WITH_DETAIL.with_graph_scale(GraphScale::Meso),
    )
    .await?;

    assert_eq!(observation.rehydration_node_id, BAD_DECISION_NODE_ID);
    assert_eq!(observation.metric.graph_scale, "meso");
    assert_eq!(observation.metric.explanation_roundtrip_fidelity, 1.0);
    assert_eq!(observation.metric.causal_reconstruction_score, 1.0);
    assert!(observation.metric.bundle_relationships > 0);
    assert_eq!(observation.metric.retry_success_hit, Some(true));
    assert_eq!(observation.metric.retry_success_rate, Some(1.0));

    emit_metric(&observation.metric)?;
    Ok(())
}

// -- Meso variants for UC2-UC4 --

#[tokio::test]
async fn meso_why_implementation_retains_rationale_under_noise()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let observation = observe_why_task_was_implemented_that_way(
        PaperUseCaseVariant::FULL_EXPLANATORY_WITH_DETAIL.with_graph_scale(GraphScale::Meso),
    )
    .await?;

    assert_eq!(observation.rationale, DECISION_TO_TASK_RATIONALE);
    assert_eq!(observation.motivation, DECISION_TO_TASK_MOTIVATION);
    assert_eq!(observation.decision_id, DECISION_NODE_ID);
    assert_eq!(observation.metric.graph_scale, "meso");
    assert_eq!(observation.metric.explanation_roundtrip_fidelity, 1.0);
    assert_eq!(observation.metric.causal_reconstruction_score, 1.0);
    assert!(
        observation
            .rendered_content
            .contains(DECISION_TO_TASK_RATIONALE)
    );

    emit_metric(&observation.metric)?;
    Ok(())
}

#[tokio::test]
async fn meso_interrupted_handoff_recovers_resume_under_noise()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let observation = observe_interrupted_handoff_use_case(
        PaperUseCaseVariant::FULL_EXPLANATORY_WITH_DETAIL.with_graph_scale(GraphScale::Meso),
    )
    .await?;

    assert_eq!(
        observation.continuation_node_id,
        HANDOFF_TASK_STARTED_NODE_ID
    );
    assert_eq!(observation.blocker_rationale, HANDOFF_BLOCKER_RATIONALE);
    assert_eq!(observation.metric.graph_scale, "meso");
    assert_eq!(observation.metric.explanation_roundtrip_fidelity, 1.0);
    assert_eq!(observation.metric.causal_reconstruction_score, 1.0);
    assert_eq!(observation.metric.retry_success_hit, Some(true));
    assert_eq!(observation.metric.retry_success_rate, Some(1.0));

    emit_metric(&observation.metric)?;
    Ok(())
}

#[tokio::test]
async fn meso_constraint_preserves_reason_under_noise() -> Result<(), Box<dyn Error + Send + Sync>>
{
    let observation = observe_constraint_under_token_pressure_use_case(
        PaperUseCaseVariant::FULL_EXPLANATORY_WITH_DETAIL.with_graph_scale(GraphScale::Meso),
        DEFAULT_TOKEN_BUDGET,
    )
    .await?;

    assert_eq!(observation.constraint_rationale, CONSTRAINT_RATIONALE);
    assert_eq!(
        observation.implementation_rationale,
        CONSTRAINT_TASK_RATIONALE
    );
    assert_eq!(observation.metric.graph_scale, "meso");
    assert_eq!(observation.metric.explanation_roundtrip_fidelity, 1.0);
    assert_eq!(observation.metric.causal_reconstruction_score, 1.0);
    assert!(observation.rendered_content.contains(CONSTRAINT_RATIONALE));

    emit_metric(&observation.metric)?;
    Ok(())
}
