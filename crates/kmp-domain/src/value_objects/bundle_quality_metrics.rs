use std::collections::BTreeMap;

use crate::error::DomainError;
use crate::model::{BundleNodeDetail, KmpBundle};
use crate::repositories::TokenEstimator;
use crate::value_objects::RelationSemanticClass;

/// Quality and efficiency metrics for a rendered context bundle.
///
/// This is a domain value object with enforced invariants:
/// - `compression_ratio` ≥ 0.0
/// - `causal_density` ∈ \[0.0, 1.0\]
/// - `noise_ratio` ∈ \[0.0, 1.0\]
/// - `detail_coverage` ∈ \[0.0, 1.0\]
///
/// Use [`BundleQualityMetrics::compute`] to derive metrics from a bundle,
/// or [`BundleQualityMetrics::new`] when reconstructing from stored values.
#[derive(Debug, Clone, PartialEq)]
pub struct BundleQualityMetrics {
    raw_equivalent_tokens: u32,
    compression_ratio: f64,
    causal_density: f64,
    noise_ratio: f64,
    detail_coverage: f64,
}

impl BundleQualityMetrics {
    /// Construct from pre-computed values with invariant validation.
    pub fn new(
        raw_equivalent_tokens: u32,
        compression_ratio: f64,
        causal_density: f64,
        noise_ratio: f64,
        detail_coverage: f64,
    ) -> Result<Self, DomainError> {
        if compression_ratio < 0.0 {
            return Err(DomainError::InvalidState(format!(
                "compression_ratio must be >= 0.0, got {compression_ratio}"
            )));
        }
        if !(0.0..=1.0).contains(&causal_density) {
            return Err(DomainError::InvalidState(format!(
                "causal_density must be in [0.0, 1.0], got {causal_density}"
            )));
        }
        if !(0.0..=1.0).contains(&noise_ratio) {
            return Err(DomainError::InvalidState(format!(
                "noise_ratio must be in [0.0, 1.0], got {noise_ratio}"
            )));
        }
        if !(0.0..=1.0).contains(&detail_coverage) {
            return Err(DomainError::InvalidState(format!(
                "detail_coverage must be in [0.0, 1.0], got {detail_coverage}"
            )));
        }
        Ok(Self {
            raw_equivalent_tokens,
            compression_ratio,
            causal_density,
            noise_ratio,
            detail_coverage,
        })
    }

    /// Compute quality metrics from a bundle and its rendering output.
    ///
    /// `rendered_tokens` is the token count of the structured rendering —
    /// the application-layer output that this metric benchmarks against.
    pub fn compute(
        bundle: &KmpBundle,
        rendered_tokens: u32,
        estimator: &dyn TokenEstimator,
    ) -> Self {
        let detail_by_node_id: BTreeMap<&str, &BundleNodeDetail> = bundle
            .node_details()
            .iter()
            .map(|d| (d.node_id(), d))
            .collect();

        let raw_equivalent_tokens =
            estimator.estimate_tokens(&raw_dump_text(bundle, &detail_by_node_id));

        let compression_ratio = if rendered_tokens > 0 {
            raw_equivalent_tokens as f64 / rendered_tokens as f64
        } else {
            1.0
        };

        let causal_density = compute_causal_density(bundle);
        let noise_ratio = compute_noise_ratio(bundle);
        let detail_coverage = compute_detail_coverage(bundle, &detail_by_node_id);

        Self {
            raw_equivalent_tokens,
            compression_ratio,
            causal_density,
            noise_ratio,
            detail_coverage,
        }
    }

    // ── Getters ─────────────────────────────────────────────────────────

    /// Token count for a flat text dump of the same data (no structure).
    pub fn raw_equivalent_tokens(&self) -> u32 {
        self.raw_equivalent_tokens
    }

    /// `raw_equivalent_tokens / rendered_token_count`. >1.0 means the
    /// structured rendering compressed vs flat text.
    pub fn compression_ratio(&self) -> f64 {
        self.compression_ratio
    }

    /// Fraction of relationships with causal/motivational/evidential
    /// semantic class (vs structural/procedural). Higher = richer signal.
    pub fn causal_density(&self) -> f64 {
        self.causal_density
    }

    /// Fraction of nodes that come from noise/distractor branches.
    /// 0.0 for clean graphs, >0 when structural noise is present.
    pub fn noise_ratio(&self) -> f64 {
        self.noise_ratio
    }

    /// Fraction of nodes that have extended detail attached.
    pub fn detail_coverage(&self) -> f64 {
        self.detail_coverage
    }
}

// ── Private domain logic ────────────────────────────────────────────────

/// Canonical flat text representation of a bundle's data.
///
/// This defines "what would a naive flat dump look like?" — the baseline
/// against which the kernel's structured rendering is measured. Both the
/// kernel and the testkit's `raw_dump.rs` must produce identical output
/// for the same data.
fn raw_dump_text(
    bundle: &KmpBundle,
    detail_by_node_id: &BTreeMap<&str, &BundleNodeDetail>,
) -> String {
    let mut raw_text = String::new();

    // Root node
    let root = bundle.root_node();
    raw_text.push_str(&format!(
        "Node: {}. Kind: {}. Summary: {}.",
        root.node_id(),
        root.node_kind(),
        root.summary()
    ));
    if let Some(detail) = detail_by_node_id.get(root.node_id()) {
        raw_text.push_str(&format!(" Detail: {}.", detail.detail()));
    }
    raw_text.push('\n');

    // Neighbor nodes
    for node in bundle.neighbor_nodes() {
        raw_text.push_str(&format!(
            "Node: {}. Kind: {}. Summary: {}.",
            node.node_id(),
            node.node_kind(),
            node.summary()
        ));
        if let Some(detail) = detail_by_node_id.get(node.node_id()) {
            raw_text.push_str(&format!(" Detail: {}.", detail.detail()));
        }
        raw_text.push('\n');
    }

    // Relationships
    for rel in bundle.relationships() {
        raw_text.push_str(&format!(
            "Relationship: {} connects to {} via {}. Semantic class: {}.",
            rel.source_node_id(),
            rel.target_node_id(),
            rel.relationship_type(),
            rel.explanation().semantic_class().as_str(),
        ));
        if let Some(r) = rel.explanation().rationale() {
            raw_text.push_str(&format!(" Rationale: {r}."));
        }
        if let Some(m) = rel.explanation().motivation() {
            raw_text.push_str(&format!(" Motivation: {m}."));
        }
        if let Some(m) = rel.explanation().method() {
            raw_text.push_str(&format!(" Method: {m}."));
        }
        if let Some(d) = rel.explanation().decision_id() {
            raw_text.push_str(&format!(" Decision: {d}."));
        }
        if let Some(c) = rel.explanation().caused_by_node_id() {
            raw_text.push_str(&format!(" Caused by: {c}."));
        }
        raw_text.push('\n');
    }

    raw_text
}

fn compute_causal_density(bundle: &KmpBundle) -> f64 {
    let total = bundle.relationships().len();
    if total == 0 {
        return 0.0;
    }
    let causal = bundle
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
    causal as f64 / total as f64
}

fn compute_noise_ratio(bundle: &KmpBundle) -> f64 {
    let total = 1 + bundle.neighbor_nodes().len(); // root + neighbors
    if total == 0 {
        return 0.0;
    }
    let noise = bundle
        .neighbor_nodes()
        .iter()
        .filter(|n| {
            let id = n.node_id();
            id.contains("noise") || id.contains("distractor")
        })
        .count();
    noise as f64 / total as f64
}

fn compute_detail_coverage(
    bundle: &KmpBundle,
    detail_by_node_id: &BTreeMap<&str, &BundleNodeDetail>,
) -> f64 {
    let total = 1 + bundle.neighbor_nodes().len();
    if total == 0 {
        return 0.0;
    }
    detail_by_node_id.len() as f64 / total as f64
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::model::{BundleNode, BundleNodeDetail, BundleRelationship, KmpBundle};
    use crate::value_objects::{
        BundleMetadata, CaseId, RelationExplanation, RelationSemanticClass, Role,
    };

    use super::BundleQualityMetrics;

    // ── Stub estimator for deterministic tests ──────────────────────────

    struct WordCountEstimator;

    impl crate::repositories::TokenEstimator for WordCountEstimator {
        fn estimate_tokens(&self, text: &str) -> u32 {
            text.split_whitespace().count() as u32
        }
        fn name(&self) -> &str {
            "word_count"
        }
    }

    // ── Constructor invariant tests ─────────────────────────────────────

    #[test]
    fn new_accepts_valid_metrics() {
        let m = BundleQualityMetrics::new(200, 1.5, 0.6, 0.1, 0.8);
        assert!(m.is_ok());
        let m = m.expect("valid metrics");
        assert_eq!(m.raw_equivalent_tokens(), 200);
        assert!((m.compression_ratio() - 1.5).abs() < 0.001);
        assert!((m.causal_density() - 0.6).abs() < 0.001);
        assert!((m.noise_ratio() - 0.1).abs() < 0.001);
        assert!((m.detail_coverage() - 0.8).abs() < 0.001);
    }

    #[test]
    fn new_rejects_negative_compression_ratio() {
        let err = BundleQualityMetrics::new(100, -0.5, 0.5, 0.0, 0.0)
            .expect_err("negative compression_ratio must fail");
        assert!(
            format!("{err}").contains("compression_ratio"),
            "error should mention compression_ratio: {err}"
        );
    }

    #[test]
    fn new_rejects_causal_density_above_one() {
        let err = BundleQualityMetrics::new(100, 1.0, 1.1, 0.0, 0.0)
            .expect_err("causal_density > 1.0 must fail");
        assert!(format!("{err}").contains("causal_density"));
    }

    #[test]
    fn new_rejects_negative_noise_ratio() {
        let err = BundleQualityMetrics::new(100, 1.0, 0.5, -0.1, 0.0)
            .expect_err("negative noise_ratio must fail");
        assert!(format!("{err}").contains("noise_ratio"));
    }

    #[test]
    fn new_rejects_detail_coverage_above_one() {
        let err = BundleQualityMetrics::new(100, 1.0, 0.5, 0.0, 1.5)
            .expect_err("detail_coverage > 1.0 must fail");
        assert!(format!("{err}").contains("detail_coverage"));
    }

    #[test]
    fn new_accepts_boundary_values() {
        assert!(BundleQualityMetrics::new(0, 0.0, 0.0, 0.0, 0.0).is_ok());
        assert!(BundleQualityMetrics::new(u32::MAX, 999.0, 1.0, 1.0, 1.0).is_ok());
    }

    // ── Compute factory tests ───────────────────────────────────────────

    fn quality_bundle() -> KmpBundle {
        KmpBundle::new(
            CaseId::new("root").expect("valid"),
            Role::new("dev").expect("valid"),
            BundleNode::new(
                "root",
                "incident",
                "Root",
                "Root summary",
                "ACTIVE",
                vec![],
                BTreeMap::new(),
            ),
            vec![
                BundleNode::new(
                    "node-a",
                    "decision",
                    "Decision A",
                    "Decision summary",
                    "ACTIVE",
                    vec![],
                    BTreeMap::new(),
                ),
                BundleNode::new(
                    "noise-1",
                    "task",
                    "Noise node",
                    "Distractor summary",
                    "ACTIVE",
                    vec![],
                    BTreeMap::new(),
                ),
            ],
            vec![
                BundleRelationship::new(
                    "root",
                    "node-a",
                    "CAUSED",
                    RelationExplanation::new(RelationSemanticClass::Causal)
                        .with_rationale("failure triggered reroute")
                        .with_caused_by_node_id("root"),
                ),
                BundleRelationship::new(
                    "root",
                    "noise-1",
                    "CONTAINS",
                    RelationExplanation::new(RelationSemanticClass::Structural),
                ),
            ],
            vec![BundleNodeDetail::new(
                "root",
                "Extended root detail",
                "hash-r",
                1,
            )],
            BundleMetadata::initial("0.1.0"),
        )
        .expect("valid")
    }

    #[test]
    fn compute_raw_equivalent_tokens_is_positive() {
        let m = BundleQualityMetrics::compute(&quality_bundle(), 100, &WordCountEstimator);
        assert!(m.raw_equivalent_tokens() > 0);
    }

    #[test]
    fn compute_compression_ratio_reflects_raw_vs_rendered() {
        let bundle = quality_bundle();
        let m = BundleQualityMetrics::compute(&bundle, 50, &WordCountEstimator);
        let expected = m.raw_equivalent_tokens() as f64 / 50.0;
        assert!(
            (m.compression_ratio() - expected).abs() < 0.001,
            "compression_ratio {:.4} != expected {:.4}",
            m.compression_ratio(),
            expected
        );
    }

    #[test]
    fn compute_compression_ratio_defaults_to_one_when_zero_rendered() {
        let bundle = quality_bundle();
        let m = BundleQualityMetrics::compute(&bundle, 0, &WordCountEstimator);
        assert!((m.compression_ratio() - 1.0).abs() < 0.001);
    }

    #[test]
    fn compute_causal_density_counts_explanatory_relations() {
        let m = BundleQualityMetrics::compute(&quality_bundle(), 100, &WordCountEstimator);
        // 1 Causal out of 2 total → 0.5
        assert!((m.causal_density() - 0.5).abs() < 0.001);
    }

    #[test]
    fn compute_noise_ratio_detects_noise_nodes() {
        let m = BundleQualityMetrics::compute(&quality_bundle(), 100, &WordCountEstimator);
        // 1 noise node out of 3 total → 1/3
        assert!((m.noise_ratio() - 1.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn compute_detail_coverage_tracks_detail_presence() {
        let m = BundleQualityMetrics::compute(&quality_bundle(), 100, &WordCountEstimator);
        // 1 detail (root) out of 3 nodes → 1/3
        assert!((m.detail_coverage() - 1.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn compute_caused_by_increases_raw_tokens() {
        let with = quality_bundle();
        let without = KmpBundle::new(
            CaseId::new("root").expect("valid"),
            Role::new("dev").expect("valid"),
            BundleNode::new(
                "root",
                "incident",
                "Root",
                "Root summary",
                "ACTIVE",
                vec![],
                BTreeMap::new(),
            ),
            vec![BundleNode::new(
                "node-a",
                "decision",
                "A",
                "A summary",
                "ACTIVE",
                vec![],
                BTreeMap::new(),
            )],
            vec![BundleRelationship::new(
                "root",
                "node-a",
                "CAUSED",
                RelationExplanation::new(RelationSemanticClass::Causal)
                    .with_rationale("failure triggered reroute"),
            )],
            vec![BundleNodeDetail::new(
                "root",
                "Extended root detail",
                "hash-r",
                1,
            )],
            BundleMetadata::initial("0.1.0"),
        )
        .expect("valid");

        let m_with = BundleQualityMetrics::compute(&with, 100, &WordCountEstimator);
        let m_without = BundleQualityMetrics::compute(&without, 100, &WordCountEstimator);
        assert!(
            m_with.raw_equivalent_tokens() > m_without.raw_equivalent_tokens(),
            "caused_by should increase raw tokens: with={} without={}",
            m_with.raw_equivalent_tokens(),
            m_without.raw_equivalent_tokens()
        );
    }

    #[test]
    fn compute_all_causal_density_is_one() {
        let bundle = KmpBundle::new(
            CaseId::new("root").expect("valid"),
            Role::new("dev").expect("valid"),
            BundleNode::new(
                "root",
                "case",
                "Root",
                "",
                "ACTIVE",
                vec![],
                BTreeMap::new(),
            ),
            vec![
                BundleNode::new("a", "task", "A", "", "ACTIVE", vec![], BTreeMap::new()),
                BundleNode::new("b", "task", "B", "", "ACTIVE", vec![], BTreeMap::new()),
            ],
            vec![
                BundleRelationship::new(
                    "root",
                    "a",
                    "CAUSED",
                    RelationExplanation::new(RelationSemanticClass::Causal),
                ),
                BundleRelationship::new(
                    "root",
                    "b",
                    "JUSTIFIED",
                    RelationExplanation::new(RelationSemanticClass::Evidential),
                ),
            ],
            Vec::new(),
            BundleMetadata::initial("0.1.0"),
        )
        .expect("valid");

        let m = BundleQualityMetrics::compute(&bundle, 100, &WordCountEstimator);
        assert!((m.causal_density() - 1.0).abs() < 0.001);
    }

    #[test]
    fn compute_no_relationships_has_zero_causal_density() {
        let bundle = KmpBundle::new(
            CaseId::new("root").expect("valid"),
            Role::new("dev").expect("valid"),
            BundleNode::new(
                "root",
                "case",
                "Root",
                "",
                "ACTIVE",
                vec![],
                BTreeMap::new(),
            ),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            BundleMetadata::initial("0.1.0"),
        )
        .expect("valid");

        let m = BundleQualityMetrics::compute(&bundle, 100, &WordCountEstimator);
        assert!(m.causal_density().abs() < 0.001);
    }
}
