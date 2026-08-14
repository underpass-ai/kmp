//! Convert a [`GeneratedSeed`] into a [`KmpBundle`] for use with
//! domain-layer quality metrics. This ensures the testkit uses the same
//! `BundleQualityMetrics::compute()` as the kernel — single source of truth.

use kmp_domain::{
    BundleMetadata, BundleNode, BundleNodeDetail, BundleQualityMetrics, BundleRelationship, CaseId,
    KmpBundle, RelationExplanation, Role, TokenEstimator,
};

use crate::dataset_generator::{GeneratedNode, GeneratedRelation, GeneratedSeed};

/// Convert a generated seed into a domain bundle.
pub fn seed_to_bundle(seed: &GeneratedSeed) -> KmpBundle {
    let case_id = CaseId::new(&seed.root.node_id).expect("seed root node_id must be valid");
    let role = Role::new("benchmark").expect("role must be valid");

    let root_node = map_node(&seed.root);

    let neighbor_nodes: Vec<BundleNode> = seed.nodes.iter().map(map_node).collect();

    let relationships: Vec<BundleRelationship> =
        seed.relations.iter().map(map_relationship).collect();

    let mut details: Vec<BundleNodeDetail> = Vec::new();
    if let Some(detail) = &seed.root.detail {
        details.push(BundleNodeDetail::new(&seed.root.node_id, detail, "seed", 1));
    }
    for node in &seed.nodes {
        if let Some(detail) = &node.detail {
            details.push(BundleNodeDetail::new(&node.node_id, detail, "seed", 1));
        }
    }

    KmpBundle::new(
        case_id,
        role,
        root_node,
        neighbor_nodes,
        relationships,
        details,
        BundleMetadata::initial("testkit"),
    )
    .expect("seed should produce a valid bundle")
}

/// Compute raw equivalent tokens for a seed using the domain VO.
///
/// This is the single source of truth — identical to what the kernel computes.
pub fn seed_raw_equivalent_tokens(seed: &GeneratedSeed, estimator: &dyn TokenEstimator) -> u32 {
    let bundle = seed_to_bundle(seed);
    BundleQualityMetrics::compute(&bundle, 0, estimator).raw_equivalent_tokens()
}

fn map_node(node: &GeneratedNode) -> BundleNode {
    BundleNode::new(
        &node.node_id,
        &node.node_kind,
        &node.title,
        &node.summary,
        "ACTIVE",
        node.labels.clone(),
        node.properties.clone(),
    )
}

fn map_relationship(rel: &GeneratedRelation) -> BundleRelationship {
    let mut explanation = RelationExplanation::new(rel.semantic_class);
    if let Some(ref r) = rel.rationale {
        explanation = explanation.with_rationale(r);
    }
    if let Some(ref m) = rel.motivation {
        explanation = explanation.with_motivation(m);
    }
    if let Some(ref m) = rel.method {
        explanation = explanation.with_method(m);
    }
    if let Some(ref d) = rel.decision_id {
        explanation = explanation.with_decision_id(d);
    }
    if let Some(ref c) = rel.caused_by_node_id {
        explanation = explanation.with_caused_by_node_id(c);
    }
    if let Some(s) = rel.sequence {
        explanation = explanation.with_sequence(s);
    }
    BundleRelationship::new(
        &rel.source_node_id,
        &rel.target_node_id,
        &rel.relation_type,
        explanation,
    )
}

#[cfg(test)]
mod tests {
    use kmp_domain::BundleQualityMetrics;

    use crate::dataset_generator::{Domain, GraphSeedConfig, generate_seed};

    use super::*;

    #[test]
    fn seed_to_bundle_preserves_node_count() {
        let seed = generate_seed(GraphSeedConfig::micro(Domain::Operations));
        let bundle = seed_to_bundle(&seed);
        assert_eq!(bundle.stats().selected_nodes() as usize, seed.total_nodes());
    }

    #[test]
    fn seed_to_bundle_preserves_relationship_count() {
        let seed = generate_seed(GraphSeedConfig::micro(Domain::Operations));
        let bundle = seed_to_bundle(&seed);
        assert_eq!(
            bundle.stats().selected_relationships() as usize,
            seed.total_relations()
        );
    }

    #[test]
    fn seed_to_bundle_preserves_details() {
        let seed = generate_seed(GraphSeedConfig::micro(Domain::Operations));
        let expected_details = std::iter::once(&seed.root)
            .chain(seed.nodes.iter())
            .filter(|n| n.detail.is_some())
            .count();
        let bundle = seed_to_bundle(&seed);
        assert_eq!(bundle.node_details().len(), expected_details);
    }

    #[test]
    fn quality_metrics_via_seed_to_bundle_are_consistent() {
        let seed = generate_seed(GraphSeedConfig::meso(Domain::Operations));
        let bundle = seed_to_bundle(&seed);
        let estimator = kmp_application::queries::cl100k_estimator::Cl100kEstimator::new();
        let metrics = BundleQualityMetrics::compute(&bundle, 0, &estimator);

        assert!(
            metrics.raw_equivalent_tokens() > 200,
            "meso should have substantial raw tokens"
        );
        assert!(
            (metrics.compression_ratio() - 1.0).abs() < 0.001,
            "compression with 0 rendered tokens should be 1.0"
        );
        assert!(
            metrics.causal_density() > 0.0,
            "meso should have causal relationships"
        );
    }
}
