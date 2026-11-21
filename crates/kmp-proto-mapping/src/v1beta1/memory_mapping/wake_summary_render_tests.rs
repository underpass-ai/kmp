//! A wake read with a summary render is the packet a structure render gives:
//! the flat sections are rendered from the same bundle exactly when the
//! state needs them, and never otherwise.
use std::collections::BTreeMap;

use kmp_application::queries::{ContextRenderOptions, render_graph_bundle_on_demand};
use kmp_application::{GetContextResult, RenderDemand};
use kmp_domain::{
    BundleMetadata, BundleNode, BundleNodeDetail, BundleRelationship, CaseId, KmpBundle, KmpMode,
    RelationExplanation, RelationSemanticClass, Role, TemporalSelection,
};

use super::{wake_response_from_result, wake_response_rendering};

fn node(id: &str, summary: &str) -> BundleNode {
    BundleNode::new(
        id,
        "decision",
        id,
        summary,
        "ACTIVE",
        Vec::new(),
        BTreeMap::new(),
    )
}

fn entry(target: &str) -> BundleRelationship {
    BundleRelationship::new(
        "timeline:main",
        target,
        "contains_entry",
        RelationExplanation::new(RelationSemanticClass::Structural)
            .with_dimension("timeline")
            .with_scope_id("timeline:main")
            .with_observed_at("2026-03-01T00:00:00Z"),
    )
}

/// An about of `memories` decisions, each explained by a causal edge to the
/// next and carrying a detail.
fn about(memories: usize) -> KmpBundle {
    let ids = (0..memories)
        .map(|index| format!("decision:{index}"))
        .collect::<Vec<_>>();
    let mut neighbours = vec![node("timeline:main", "")];
    let mut relationships = Vec::new();
    let mut details = Vec::new();
    for (index, id) in ids.iter().enumerate() {
        neighbours.push(node(id, &format!("The pump {index} was replaced")));
        relationships.push(entry(id));
        details.push(BundleNodeDetail::new(
            id.as_str(),
            format!("Pump {index} failed during the night shift."),
            "hash",
            1,
        ));
        if let Some(next) = ids.get(index + 1) {
            relationships.push(BundleRelationship::new(
                id.as_str(),
                next.as_str(),
                "triggers",
                RelationExplanation::new(RelationSemanticClass::Causal)
                    .with_rationale("the replacement exposed the next fault"),
            ));
        }
    }
    KmpBundle::new(
        CaseId::new("project:kmp").expect("case id"),
        Role::new("resumer").expect("role"),
        node("project:kmp", "Keep the plant running"),
        neighbours,
        relationships,
        details,
        BundleMetadata::initial("test"),
    )
    .expect("bundle")
}

fn options(budget: Option<u32>) -> ContextRenderOptions {
    ContextRenderOptions {
        token_budget: budget,
        rehydration_mode: KmpMode::ResumeFocused,
        ..ContextRenderOptions::default()
    }
}

fn result(
    bundle: KmpBundle,
    options: &ContextRenderOptions,
    demand: RenderDemand,
) -> GetContextResult {
    GetContextResult {
        read_revision: None,
        rendered: render_graph_bundle_on_demand(&bundle, options, demand),
        bundle,
        requested_scopes: Vec::new(),
        served_at: std::time::SystemTime::UNIX_EPOCH,
        timing: None,
    }
}

#[test]
fn a_summary_wake_renders_the_sections_its_state_needs_and_equals_a_structure_wake() {
    for memories in [0, 1, 3, 4] {
        for budget in [None, Some(40), Some(1600)] {
            let options = options(budget);
            let structure = wake_response_from_result(
                "resume",
                None,
                result(about(memories), &options, RenderDemand::Structure),
                &TemporalSelection::Frontier,
            )
            .expect("structure packet");
            let rendered = std::cell::Cell::new(0);
            let sections = |bundle: &KmpBundle| {
                rendered.set(rendered.get() + 1);
                render_graph_bundle_on_demand(bundle, &options, RenderDemand::Structure)
            };
            let summary = wake_response_rendering(
                "resume",
                None,
                result(about(memories), &options, RenderDemand::Summary),
                &TemporalSelection::Frontier,
                None,
                Some(&sections),
            )
            .expect("summary packet");
            assert_eq!(summary, structure, "{memories} memories, budget {budget:?}");
            assert_eq!(
                rendered.get(),
                1,
                "{memories} memories leave room for sections"
            );
        }
    }
}

#[test]
fn a_summary_wake_whose_live_memories_fill_the_state_renders_nothing_more() {
    for budget in [None, Some(1600)] {
        let options = options(budget);
        let structure = wake_response_from_result(
            "resume",
            None,
            result(about(7), &options, RenderDemand::Structure),
            &TemporalSelection::Frontier,
        )
        .expect("structure packet");
        let sections = |_: &KmpBundle| -> kmp_application::RenderedContext {
            panic!("live memories fill the state: no section is read")
        };
        let summary = wake_response_rendering(
            "resume",
            None,
            result(about(7), &options, RenderDemand::Summary),
            &TemporalSelection::Frontier,
            None,
            Some(&sections),
        )
        .expect("summary packet");
        assert_eq!(summary, structure, "budget {budget:?}");
    }
}
