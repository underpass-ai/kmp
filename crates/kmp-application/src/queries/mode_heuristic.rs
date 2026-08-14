use kmp_domain::{KmpBundle, KmpMode, RelationSemanticClass};

use crate::queries::EndpointHint;

/// Resolves `Auto` mode into a concrete mode based on token pressure,
/// causal density, focus presence, and endpoint type.
///
/// When mode is explicit (not Auto), it passes through unchanged.
/// When Auto, the heuristic considers:
/// 1. Token pressure (budget / nodes) — tight budgets favor ResumeFocused
/// 2. Causal density — high explanatory ratio preserves rationale; very low density prunes even at generous budgets
/// 3. Focus presence — scoped paths tolerate more pruning (threshold 30 → 60)
/// 4. Endpoint type — sessions need richer context (threshold → 15)
pub(crate) fn resolve_mode(
    explicit_mode: KmpMode,
    bundle: &KmpBundle,
    token_budget: Option<u32>,
    focus_node_id: Option<&str>,
    endpoint_hint: EndpointHint,
) -> KmpMode {
    match explicit_mode {
        KmpMode::Auto => auto_detect(bundle, token_budget, focus_node_id, endpoint_hint),
        concrete => concrete,
    }
}

/// Default tokens-per-node threshold (GetContext without focus).
const TOKENS_PER_NODE_THRESHOLD: u32 = 30;

/// When a focus path is set, scoped context benefits from pruning even at
/// moderate budgets. Double the threshold.
const FOCUSED_TOKENS_PER_NODE_THRESHOLD: u32 = 60;

/// Sessions serve multi-role snapshots and need richer context. Lower threshold
/// means we stay in ReasonPreserving longer.
const SESSION_TOKENS_PER_NODE_THRESHOLD: u32 = 15;

/// Causal density above which we keep ReasonPreserving even under token pressure.
const CAUSAL_DENSITY_PRESERVE_THRESHOLD: f64 = 0.5;

/// Causal density below which we switch to ResumeFocused even at generous budgets.
/// Structural-heavy graphs have nothing worth preserving in ReasonPreserving mode.
const STRUCTURAL_OVERRIDE_DENSITY: f64 = 0.2;

fn auto_detect(
    bundle: &KmpBundle,
    token_budget: Option<u32>,
    focus_node_id: Option<&str>,
    endpoint_hint: EndpointHint,
) -> KmpMode {
    let Some(budget) = token_budget else {
        return KmpMode::ReasonPreserving;
    };

    let total_nodes = bundle.stats().selected_nodes();
    if total_nodes == 0 {
        return KmpMode::ReasonPreserving;
    }

    let tokens_per_node = budget / total_nodes;
    let causal_density = bundle_causal_density(bundle);

    let effective_threshold = match endpoint_hint {
        EndpointHint::SessionSnapshot => SESSION_TOKENS_PER_NODE_THRESHOLD,
        _ if focus_node_id.is_some() => FOCUSED_TOKENS_PER_NODE_THRESHOLD,
        _ => TOKENS_PER_NODE_THRESHOLD,
    };

    if tokens_per_node >= effective_threshold {
        // Budget is generous relative to endpoint type.
        // Structural-heavy graphs still benefit from pruning.
        if causal_density < STRUCTURAL_OVERRIDE_DENSITY {
            return KmpMode::ResumeFocused;
        }
        return KmpMode::ReasonPreserving;
    }

    // Budget is tight — preserve rationale only if density justifies it.
    if causal_density >= CAUSAL_DENSITY_PRESERVE_THRESHOLD {
        KmpMode::ReasonPreserving
    } else {
        KmpMode::ResumeFocused
    }
}

/// Fraction of relationships with explanatory semantic class (causal, motivational, evidential).
fn bundle_causal_density(bundle: &KmpBundle) -> f64 {
    let total = bundle.relationships().len();
    if total == 0 {
        return 0.0;
    }
    let explanatory = bundle
        .relationships()
        .iter()
        .filter(|r| {
            matches!(
                r.explanation().semantic_class(),
                RelationSemanticClass::Causal
                    | RelationSemanticClass::Motivational
                    | RelationSemanticClass::Evidential
            )
        })
        .count();
    explanatory as f64 / total as f64
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use kmp_domain::{
        BundleMetadata, BundleNode, BundleRelationship, CaseId, KmpBundle, KmpMode,
        RelationExplanation, RelationSemanticClass, Role,
    };

    use super::resolve_mode;
    use crate::queries::EndpointHint;

    fn bundle_with_nodes(count: usize) -> KmpBundle {
        let root = BundleNode::new(
            "case",
            "case",
            "Root",
            "",
            "ACTIVE",
            vec![],
            BTreeMap::new(),
        );
        let neighbors: Vec<_> = (0..count.saturating_sub(1))
            .map(|i| {
                BundleNode::new(
                    format!("n{i}"),
                    "task",
                    format!("N{i}"),
                    "",
                    "ACTIVE",
                    vec![],
                    BTreeMap::new(),
                )
            })
            .collect();
        KmpBundle::new(
            CaseId::new("case").expect("valid"),
            Role::new("dev").expect("valid"),
            root,
            neighbors,
            Vec::new(),
            Vec::new(),
            BundleMetadata::initial("0.1.0"),
        )
        .expect("valid")
    }

    fn bundle_with_causal_relations(node_count: usize, causal_count: usize) -> KmpBundle {
        let root = BundleNode::new(
            "case",
            "case",
            "Root",
            "",
            "ACTIVE",
            vec![],
            BTreeMap::new(),
        );
        let neighbors: Vec<_> = (0..node_count.saturating_sub(1))
            .map(|i| {
                BundleNode::new(
                    format!("n{i}"),
                    "task",
                    format!("N{i}"),
                    "",
                    "ACTIVE",
                    vec![],
                    BTreeMap::new(),
                )
            })
            .collect();
        let mut relationships = Vec::new();
        for i in 0..node_count.saturating_sub(1) {
            let class = if i < causal_count {
                RelationSemanticClass::Causal
            } else {
                RelationSemanticClass::Structural
            };
            relationships.push(BundleRelationship::new(
                "case",
                format!("n{i}"),
                "RELATES",
                RelationExplanation::new(class),
            ));
        }
        KmpBundle::new(
            CaseId::new("case").expect("valid"),
            Role::new("dev").expect("valid"),
            root,
            neighbors,
            relationships,
            Vec::new(),
            BundleMetadata::initial("0.1.0"),
        )
        .expect("valid")
    }

    // ── v2 tests (updated with new params, same assertions) ──

    #[test]
    fn auto_selects_resume_focused_when_budget_tight_and_structural() {
        let bundle = bundle_with_nodes(49);
        let mode = resolve_mode(
            KmpMode::Auto,
            &bundle,
            Some(512),
            None,
            EndpointHint::Neighborhood,
        );
        assert_eq!(mode, KmpMode::ResumeFocused);
    }

    #[test]
    fn auto_keeps_reason_preserving_when_budget_tight_but_high_causal_density() {
        let bundle = bundle_with_causal_relations(49, 40);
        let mode = resolve_mode(
            KmpMode::Auto,
            &bundle,
            Some(512),
            None,
            EndpointHint::Neighborhood,
        );
        assert_eq!(mode, KmpMode::ReasonPreserving);
    }

    #[test]
    fn auto_selects_resume_focused_when_budget_tight_and_low_causal_density() {
        let bundle = bundle_with_causal_relations(49, 5);
        let mode = resolve_mode(
            KmpMode::Auto,
            &bundle,
            Some(512),
            None,
            EndpointHint::Neighborhood,
        );
        assert_eq!(mode, KmpMode::ResumeFocused);
    }

    #[test]
    fn auto_selects_reason_preserving_when_no_budget() {
        let bundle = bundle_with_nodes(49);
        let mode = resolve_mode(
            KmpMode::Auto,
            &bundle,
            None,
            None,
            EndpointHint::Neighborhood,
        );
        assert_eq!(mode, KmpMode::ReasonPreserving);
    }

    #[test]
    fn explicit_mode_passes_through() {
        let bundle = bundle_with_nodes(49);
        let mode = resolve_mode(
            KmpMode::ResumeFocused,
            &bundle,
            Some(4096),
            None,
            EndpointHint::Neighborhood,
        );
        assert_eq!(mode, KmpMode::ResumeFocused);
    }

    #[test]
    fn auto_selects_resume_focused_for_single_node_tight_budget() {
        let bundle = bundle_with_nodes(1);
        let mode = resolve_mode(
            KmpMode::Auto,
            &bundle,
            Some(10),
            None,
            EndpointHint::Neighborhood,
        );
        assert_eq!(mode, KmpMode::ResumeFocused);
    }

    // ── Enrichment 1: focus presence ──

    #[test]
    fn focus_biases_toward_resume_focused_at_moderate_budget() {
        // 21 nodes, 1000 tokens → 47 tok/node. Without focus: >= 30 → ReasonPreserving.
        // With focus: < 60 → falls through to density check → 0 density → ResumeFocused.
        let bundle = bundle_with_nodes(21);
        let mode = resolve_mode(
            KmpMode::Auto,
            &bundle,
            Some(1000),
            Some("n1"),
            EndpointHint::Neighborhood,
        );
        assert_eq!(mode, KmpMode::ResumeFocused);
    }

    #[test]
    fn focus_keeps_reason_preserving_at_generous_budget() {
        // 10 nodes, 1000 tokens → 100 tok/node. Even with focus (threshold 60), 100 >= 60.
        // But 0 causal density → structural override → ResumeFocused.
        // Need causal relations to stay ReasonPreserving.
        let bundle = bundle_with_causal_relations(10, 5); // 55% causal
        let mode = resolve_mode(
            KmpMode::Auto,
            &bundle,
            Some(1000),
            Some("n1"),
            EndpointHint::Neighborhood,
        );
        assert_eq!(mode, KmpMode::ReasonPreserving);
    }

    #[test]
    fn focus_does_not_affect_no_budget_case() {
        let bundle = bundle_with_nodes(49);
        let mode = resolve_mode(
            KmpMode::Auto,
            &bundle,
            None,
            Some("n1"),
            EndpointHint::Neighborhood,
        );
        assert_eq!(mode, KmpMode::ReasonPreserving);
    }

    // ── Enrichment 2: endpoint type ──

    #[test]
    fn session_snapshot_keeps_reason_preserving_at_moderate_pressure() {
        // 49 nodes, 1000 tokens → 20 tok/node. Normal threshold (30): tight → density check.
        // Session threshold (15): 20 >= 15 → generous. With causal relations → ReasonPreserving.
        let bundle = bundle_with_causal_relations(49, 30); // 62% causal
        let mode = resolve_mode(
            KmpMode::Auto,
            &bundle,
            Some(1000),
            None,
            EndpointHint::SessionSnapshot,
        );
        assert_eq!(mode, KmpMode::ReasonPreserving);
    }

    #[test]
    fn session_snapshot_falls_to_resume_focused_at_extreme_pressure() {
        // 49 nodes, 200 tokens → 4 tok/node. Even session threshold (15): 4 < 15 → tight.
        // Low density → ResumeFocused.
        let bundle = bundle_with_nodes(49);
        let mode = resolve_mode(
            KmpMode::Auto,
            &bundle,
            Some(200),
            None,
            EndpointHint::SessionSnapshot,
        );
        assert_eq!(mode, KmpMode::ResumeFocused);
    }

    #[test]
    fn focused_path_hint_activates_focus_threshold() {
        // 21 nodes, 1000 tokens → 47 tok/node. FocusedPath uses threshold 60.
        // 47 < 60 → tight → 0 density → ResumeFocused.
        let bundle = bundle_with_nodes(21);
        let mode = resolve_mode(
            KmpMode::Auto,
            &bundle,
            Some(1000),
            None,
            EndpointHint::FocusedPath,
        );
        assert_eq!(mode, KmpMode::ResumeFocused);
    }

    // ── Enrichment 3: relation distribution ──

    #[test]
    fn generous_budget_structural_graph_switches_to_resume_focused() {
        // 21 nodes, 4096 tokens → 195 tok/node. Very generous.
        // But 0 relations → density 0 < 0.2 → structural override → ResumeFocused.
        let bundle = bundle_with_nodes(21);
        let mode = resolve_mode(
            KmpMode::Auto,
            &bundle,
            Some(4096),
            None,
            EndpointHint::Neighborhood,
        );
        assert_eq!(mode, KmpMode::ResumeFocused);
    }

    #[test]
    fn generous_budget_causal_graph_stays_reason_preserving() {
        // 21 nodes, 4096 tokens → 195 tok/node. 50% causal density → above 0.2 → ReasonPreserving.
        let bundle = bundle_with_causal_relations(21, 10); // 50% causal
        let mode = resolve_mode(
            KmpMode::Auto,
            &bundle,
            Some(4096),
            None,
            EndpointHint::Neighborhood,
        );
        assert_eq!(mode, KmpMode::ReasonPreserving);
    }

    #[test]
    fn no_budget_stays_reason_preserving_even_when_all_structural() {
        let bundle = bundle_with_nodes(49);
        let mode = resolve_mode(
            KmpMode::Auto,
            &bundle,
            None,
            None,
            EndpointHint::Neighborhood,
        );
        assert_eq!(mode, KmpMode::ReasonPreserving);
    }
}
