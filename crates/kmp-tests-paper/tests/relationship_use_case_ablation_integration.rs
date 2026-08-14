#![cfg(feature = "container-tests")]

use std::error::Error;

use kmp_tests_shared::metrics::emit_metric;
use kmp_tests_shared::metrics::use_case_harness::{
    DEFAULT_TOKEN_BUDGET, LOW_TOKEN_BUDGET, PaperUseCaseVariant, STRUCTURAL_LOW_TOKEN_BUDGET,
    observe_constraint_under_token_pressure_use_case, observe_failure_diagnosis_use_case,
    observe_interrupted_handoff_use_case, observe_why_task_was_implemented_that_way,
};

#[tokio::test]
async fn failure_diagnosis_degrades_without_explanatory_relations_or_detail()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let baseline =
        observe_failure_diagnosis_use_case(PaperUseCaseVariant::FULL_EXPLANATORY_WITH_DETAIL)
            .await?;
    let structural_only =
        observe_failure_diagnosis_use_case(PaperUseCaseVariant::STRUCTURAL_ONLY_WITH_DETAIL)
            .await?;
    let without_detail =
        observe_failure_diagnosis_use_case(PaperUseCaseVariant::FULL_EXPLANATORY_WITHOUT_DETAIL)
            .await?;
    let detail_only =
        observe_failure_diagnosis_use_case(PaperUseCaseVariant::DETAIL_ONLY_WITH_DETAIL).await?;

    assert!(
        baseline.metric.explanation_roundtrip_fidelity
            > structural_only.metric.explanation_roundtrip_fidelity
    );
    assert!(
        baseline.metric.causal_reconstruction_score
            > structural_only.metric.causal_reconstruction_score
    );
    assert!(
        baseline.metric.detail_roundtrip_fidelity > without_detail.metric.detail_roundtrip_fidelity
    );
    assert!(
        baseline.metric.causal_reconstruction_score
            > without_detail.metric.causal_reconstruction_score
    );
    assert!(
        detail_only.metric.causal_reconstruction_score
            > structural_only.metric.causal_reconstruction_score
    );
    assert!(
        baseline.metric.causal_reconstruction_score
            > detail_only.metric.causal_reconstruction_score
    );
    assert_eq!(baseline.metric.retry_success_hit, Some(true));
    assert_eq!(baseline.metric.retry_success_rate, Some(1.0));
    assert_eq!(structural_only.metric.retry_success_hit, Some(false));
    assert_eq!(detail_only.metric.retry_success_hit, Some(false));
    assert_eq!(without_detail.metric.retry_success_hit, Some(true));
    assert!(!structural_only.metric.rendered_contains_expected_rationale);
    assert!(
        !structural_only
            .metric
            .rendered_contains_expected_decision_reference
    );
    assert!(!without_detail.metric.rendered_contains_expected_detail);
    assert!(detail_only.metric.rendered_contains_expected_rationale);
    assert!(
        detail_only
            .metric
            .rendered_contains_expected_decision_reference
    );

    emit_metric(&baseline.metric)?;
    emit_metric(&structural_only.metric)?;
    emit_metric(&without_detail.metric)?;
    emit_metric(&detail_only.metric)?;
    Ok(())
}

#[tokio::test]
async fn implementation_trace_degrades_without_explanatory_relations_or_detail()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let baseline = observe_why_task_was_implemented_that_way(
        PaperUseCaseVariant::FULL_EXPLANATORY_WITH_DETAIL,
    )
    .await?;
    let structural_only =
        observe_why_task_was_implemented_that_way(PaperUseCaseVariant::STRUCTURAL_ONLY_WITH_DETAIL)
            .await?;
    let without_detail = observe_why_task_was_implemented_that_way(
        PaperUseCaseVariant::FULL_EXPLANATORY_WITHOUT_DETAIL,
    )
    .await?;
    let detail_only =
        observe_why_task_was_implemented_that_way(PaperUseCaseVariant::DETAIL_ONLY_WITH_DETAIL)
            .await?;

    assert!(
        baseline.metric.explanation_roundtrip_fidelity
            > structural_only.metric.explanation_roundtrip_fidelity
    );
    assert!(
        baseline.metric.causal_reconstruction_score
            > structural_only.metric.causal_reconstruction_score
    );
    assert!(
        baseline.metric.detail_roundtrip_fidelity > without_detail.metric.detail_roundtrip_fidelity
    );
    assert!(
        baseline.metric.causal_reconstruction_score
            > without_detail.metric.causal_reconstruction_score
    );
    assert!(
        detail_only.metric.causal_reconstruction_score
            > structural_only.metric.causal_reconstruction_score
    );
    assert!(
        baseline.metric.causal_reconstruction_score
            > detail_only.metric.causal_reconstruction_score
    );
    assert_eq!(baseline.metric.retry_success_hit, None);
    assert_eq!(baseline.metric.retry_success_rate, None);
    assert_eq!(structural_only.metric.retry_success_hit, None);
    assert_eq!(detail_only.metric.retry_success_hit, None);
    assert_eq!(without_detail.metric.retry_success_hit, None);
    assert!(!structural_only.metric.rendered_contains_expected_rationale);
    assert!(
        !structural_only
            .metric
            .rendered_contains_expected_decision_reference
    );
    assert!(!without_detail.metric.rendered_contains_expected_detail);
    assert!(detail_only.metric.rendered_contains_expected_rationale);
    assert!(
        detail_only
            .metric
            .rendered_contains_expected_decision_reference
    );

    emit_metric(&baseline.metric)?;
    emit_metric(&structural_only.metric)?;
    emit_metric(&without_detail.metric)?;
    emit_metric(&detail_only.metric)?;
    Ok(())
}

#[tokio::test]
async fn handoff_trace_degrades_without_explanatory_relations_or_detail()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let baseline =
        observe_interrupted_handoff_use_case(PaperUseCaseVariant::FULL_EXPLANATORY_WITH_DETAIL)
            .await?;
    let structural_only =
        observe_interrupted_handoff_use_case(PaperUseCaseVariant::STRUCTURAL_ONLY_WITH_DETAIL)
            .await?;
    let without_detail =
        observe_interrupted_handoff_use_case(PaperUseCaseVariant::FULL_EXPLANATORY_WITHOUT_DETAIL)
            .await?;
    let detail_only =
        observe_interrupted_handoff_use_case(PaperUseCaseVariant::DETAIL_ONLY_WITH_DETAIL).await?;

    assert!(
        baseline.metric.explanation_roundtrip_fidelity
            > structural_only.metric.explanation_roundtrip_fidelity
    );
    assert!(
        baseline.metric.causal_reconstruction_score
            > structural_only.metric.causal_reconstruction_score
    );
    assert!(
        baseline.metric.detail_roundtrip_fidelity > without_detail.metric.detail_roundtrip_fidelity
    );
    assert!(
        baseline.metric.causal_reconstruction_score
            > without_detail.metric.causal_reconstruction_score
    );
    assert!(
        detail_only.metric.causal_reconstruction_score
            > structural_only.metric.causal_reconstruction_score
    );
    assert!(
        baseline.metric.causal_reconstruction_score
            > detail_only.metric.causal_reconstruction_score
    );
    assert_eq!(baseline.metric.retry_success_hit, Some(true));
    assert_eq!(baseline.metric.retry_success_rate, Some(1.0));
    assert_eq!(structural_only.metric.retry_success_hit, Some(false));
    assert_eq!(detail_only.metric.retry_success_hit, Some(false));
    assert_eq!(without_detail.metric.retry_success_hit, Some(true));
    assert!(!structural_only.metric.rendered_contains_expected_rationale);
    assert!(
        !structural_only
            .metric
            .rendered_contains_expected_decision_reference
    );
    assert!(!without_detail.metric.rendered_contains_expected_detail);
    assert!(detail_only.metric.rendered_contains_expected_rationale);
    assert!(
        detail_only
            .metric
            .rendered_contains_expected_decision_reference
    );

    emit_metric(&baseline.metric)?;
    emit_metric(&structural_only.metric)?;
    emit_metric(&without_detail.metric)?;
    emit_metric(&detail_only.metric)?;
    Ok(())
}

#[tokio::test]
async fn constraint_reason_under_token_pressure_degrades_with_budget_and_structural_edges()
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
    let structural_low_budget = observe_constraint_under_token_pressure_use_case(
        PaperUseCaseVariant::STRUCTURAL_ONLY_WITH_DETAIL,
        STRUCTURAL_LOW_TOKEN_BUDGET,
    )
    .await?;

    assert!(
        baseline.metric.detail_roundtrip_fidelity > low_budget.metric.detail_roundtrip_fidelity
    );
    assert!(
        low_budget.metric.causal_reconstruction_score
            > structural_low_budget.metric.causal_reconstruction_score
    );
    assert_eq!(low_budget.metric.dominant_reason_hit, Some(true));
    assert_eq!(
        structural_low_budget.metric.dominant_reason_hit,
        Some(false)
    );
    assert_eq!(low_budget.metric.causal_reconstruction_score, 1.0);
    assert!(
        low_budget.metric.explanation_roundtrip_fidelity
            > structural_low_budget.metric.explanation_roundtrip_fidelity
    );
    assert_eq!(low_budget.metric.detail_roundtrip_fidelity, 0.0);
    assert_eq!(structural_low_budget.metric.detail_roundtrip_fidelity, 0.0);
    assert!(low_budget.metric.rendered_token_count <= LOW_TOKEN_BUDGET);
    assert!(structural_low_budget.metric.rendered_token_count <= STRUCTURAL_LOW_TOKEN_BUDGET);

    emit_metric(&baseline.metric)?;
    emit_metric(&low_budget.metric)?;
    emit_metric(&structural_low_budget.metric)?;
    Ok(())
}
