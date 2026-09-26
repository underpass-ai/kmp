//! A render asked for less than everything is the same text, measured less.
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use kmp_domain::{
    BundleMetadata, BundleNode, BundleNodeDetail, BundleQualityMetrics, BundleRelationship, CaseId,
    KmpBundle, KmpMode, RelationExplanation, RelationSemanticClass, ResolutionTier, Role,
    TokenEstimator,
};

use crate::queries::cl100k_estimator::Cl100kEstimator;
use crate::queries::render_graph_bundle::render_graph_bundle_for_demand;
use crate::queries::{
    ContextRenderOptions, RenderDemand, RenderedContext, render_graph_bundle_on_demand,
    render_graph_bundle_with_options,
};

/// Counts how many texts a render tokenizes.
struct CountingEstimator {
    inner: Cl100kEstimator,
    calls: AtomicUsize,
}

impl CountingEstimator {
    fn new() -> Self {
        Self {
            inner: Cl100kEstimator::new(),
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }
}

impl TokenEstimator for CountingEstimator {
    fn estimate_tokens(&self, text: &str) -> u32 {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.inner.estimate_tokens(text)
    }

    fn name(&self) -> &str {
        self.inner.name()
    }
}

/// An about large enough that a small budget drops sections of every tier.
fn wide_bundle() -> KmpBundle {
    let neighbours = (0..24)
        .map(|index| {
            BundleNode::new(
                format!("entry-{index}"),
                if index % 3 == 0 { "decision" } else { "fact" },
                format!("Entry {index}"),
                format!("The valve {index} froze during the night shift and was replaced"),
                "ACTIVE",
                vec![],
                BTreeMap::new(),
            )
        })
        .collect::<Vec<_>>();
    let relationships = (0..24)
        .map(|index| {
            let class = match index % 3 {
                0 => RelationSemanticClass::Causal,
                1 => RelationSemanticClass::Evidential,
                _ => RelationSemanticClass::Structural,
            };
            BundleRelationship::new(
                "case-wide",
                format!("entry-{index}"),
                if index % 3 == 0 {
                    "caused_by"
                } else {
                    "contains_entry"
                },
                RelationExplanation::new(class)
                    .with_rationale(format!("because the pressure log {index} says so")),
            )
        })
        .collect::<Vec<_>>();
    let details = (0..24)
        .step_by(2)
        .map(|index| {
            BundleNodeDetail::new(
                format!("entry-{index}"),
                format!("Detail {index}: the replacement part came from the reserve stock."),
                format!("hash-{index}"),
                1,
            )
        })
        .collect::<Vec<_>>();
    KmpBundle::new(
        CaseId::new("case-wide").expect("case id is valid"),
        Role::new("resumer").expect("role is valid"),
        BundleNode::new(
            "case-wide",
            "about",
            "Wide about",
            "Everything the plant recorded",
            "ACTIVE",
            vec![],
            BTreeMap::new(),
        ),
        neighbours,
        relationships,
        details,
        BundleMetadata::initial("0.1.0"),
    )
    .expect("bundle should be valid")
}

fn options(token_budget: Option<u32>, mode: KmpMode) -> ContextRenderOptions {
    ContextRenderOptions {
        token_budget,
        rehydration_mode: mode,
        ..Default::default()
    }
}

fn option_grid() -> Vec<ContextRenderOptions> {
    let mut grid = Vec::new();
    for budget in [None, Some(10), Some(120), Some(1600), Some(100_000)] {
        for mode in [
            KmpMode::Auto,
            KmpMode::ReasonPreserving,
            KmpMode::ResumeFocused,
        ] {
            grid.push(options(budget, mode));
            grid.push(ContextRenderOptions {
                max_tier: Some(ResolutionTier::L1CausalSpine),
                ..options(budget, mode)
            });
        }
    }
    grid
}

/// Everything a projection reads off a render: the text and its identity.
fn text_of(rendered: &RenderedContext) -> impl PartialEq + std::fmt::Debug {
    (
        rendered.content.clone(),
        rendered.content_hash.clone(),
        rendered
            .sections
            .iter()
            .map(|section| (section.content.clone(), section.source_id.clone()))
            .collect::<Vec<_>>(),
        rendered
            .tiers
            .iter()
            .map(|tier| {
                (
                    tier.tier,
                    tier.content.clone(),
                    tier.sections
                        .iter()
                        .map(|section| (section.content.clone(), section.source_id.clone()))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>(),
        rendered.truncation.clone(),
        rendered.resolved_mode,
    )
}

#[test]
fn a_structure_render_is_the_measured_text_byte_for_byte() {
    let bundle = wide_bundle();
    for options in option_grid() {
        let measured = render_graph_bundle_on_demand(&bundle, &options, RenderDemand::Measured);
        let structure = render_graph_bundle_on_demand(&bundle, &options, RenderDemand::Structure);
        assert_eq!(text_of(&structure), text_of(&measured), "{options:?}");
        assert_eq!(structure.demand, RenderDemand::Structure);
        assert_eq!(structure.quality, BundleQualityMetrics::unmeasured());
        assert_eq!(structure.token_count, 0);
    }
}

#[test]
fn a_measured_render_is_the_default_render_and_counts_what_it_carries() {
    let bundle = wide_bundle();
    let estimator = Cl100kEstimator::new();
    for options in option_grid() {
        let measured = render_graph_bundle_on_demand(&bundle, &options, RenderDemand::Measured);
        assert_eq!(
            measured,
            render_graph_bundle_with_options(&bundle, &options)
        );
        assert_eq!(measured.demand, RenderDemand::Measured);
        assert_eq!(
            measured.token_count,
            estimator.estimate_tokens(&measured.content)
        );
        // A count the budget took once is the count the section reports.
        for section in &measured.sections {
            assert_eq!(
                section.token_count,
                estimator.estimate_tokens(&section.content),
                "{options:?}"
            );
        }
        for tier in &measured.tiers {
            assert_eq!(tier.token_count, estimator.estimate_tokens(&tier.content));
        }
        if let Some(truncation) = &measured.truncation {
            assert_eq!(
                truncation.sections_kept as usize,
                measured.sections.len(),
                "{options:?}"
            );
        }
    }
}

#[test]
fn a_skipped_render_carries_no_text_and_no_measurement() {
    let bundle = wide_bundle();
    let estimator = CountingEstimator::new();
    let skipped = render_graph_bundle_for_demand(
        &bundle,
        &options(Some(1600), KmpMode::Auto),
        &estimator,
        RenderDemand::Skip,
    );
    assert_eq!(
        estimator.calls(),
        0,
        "nothing reads it, nothing is tokenized"
    );
    assert_eq!(skipped.demand, RenderDemand::Skip);
    assert!(skipped.content.is_empty());
    assert!(skipped.sections.is_empty());
    assert!(skipped.tiers.is_empty());
    assert!(skipped.truncation.is_none());
    assert_eq!(skipped.token_count, 0);
    assert_eq!(skipped.quality, BundleQualityMetrics::unmeasured());
    assert_eq!(
        skipped.resolved_mode,
        render_graph_bundle_on_demand(
            &bundle,
            &options(Some(1600), KmpMode::Auto),
            RenderDemand::Measured
        )
        .resolved_mode
    );
}

#[test]
fn an_unbudgeted_structure_render_tokenizes_nothing() {
    let bundle = wide_bundle();
    let estimator = CountingEstimator::new();
    let structure = render_graph_bundle_for_demand(
        &bundle,
        &options(None, KmpMode::ReasonPreserving),
        &estimator,
        RenderDemand::Structure,
    );
    assert!(!structure.sections.is_empty());
    assert_eq!(estimator.calls(), 0, "no budget decision needs a count");
}

#[test]
fn a_structure_render_tokenizes_less_than_a_measured_one() {
    let bundle = wide_bundle();
    let budgeted = options(Some(120), KmpMode::ReasonPreserving);
    let measured = CountingEstimator::new();
    render_graph_bundle_for_demand(&bundle, &budgeted, &measured, RenderDemand::Measured);
    let structure = CountingEstimator::new();
    let rendered =
        render_graph_bundle_for_demand(&bundle, &budgeted, &structure, RenderDemand::Structure);
    assert!(rendered.truncation.is_some());
    // The budget counts every flat section once; the tiers count until they
    // fill. Nothing else is tokenized.
    let flat = rendered.truncation.as_ref().map_or(0, |truncation| {
        (truncation.sections_kept + truncation.sections_dropped) as usize
    });
    let tiered = rendered
        .tiers
        .iter()
        .map(|tier| tier.sections.len() + 1)
        .sum::<usize>();
    assert!(structure.calls() <= flat + tiered, "{}", structure.calls());
    assert!(
        structure.calls() < measured.calls(),
        "structure {} measured {}",
        structure.calls(),
        measured.calls()
    );
}
