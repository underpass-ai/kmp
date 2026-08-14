//! Parameterized graph seed generator for benchmark evaluation.
//!
//! Generates NATS projection event payloads from a config, enabling
//! micro/meso/stress graph scales without hand-written fixtures.

use std::collections::BTreeMap;

use kmp_domain::RelationSemanticClass;

/// Configuration for a generated graph seed.
#[derive(Debug, Clone)]
pub struct GraphSeedConfig {
    /// Depth of the causal chain from root to leaf.
    pub chain_length: usize,
    /// Number of distractor branches per decision node.
    pub noise_branches: usize,
    /// Fraction of nodes that have extended detail (0.0 - 1.0).
    pub detail_density: f64,
    /// Mix of relation types in the graph.
    pub relation_mix: RelationMix,
    /// Domain vocabulary for node kinds and rationale text.
    pub domain: Domain,
    /// Prefix for all node IDs.
    pub id_prefix: String,
    /// How realistic the noise branches are.
    pub noise_mode: NoiseMode,
    /// Seed for varying graph structure. Different seeds with the same
    /// config produce structurally different graphs (rotated kind order,
    /// different rationale text suffixes). Enables within-condition
    /// variance estimation for statistical confidence intervals.
    pub seed: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationMix {
    /// All relationships carry full explanatory metadata.
    Explanatory,
    /// Relationships are structural only (no rationale, motivation, etc).
    Structural,
    /// Mix of explanatory and structural.
    Mixed,
}

/// Controls how realistic the noise branches are.
///
/// Each mode isolates a different confusion vector so the benchmark can
/// diagnose which types of noise actually degrade LLM performance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NoiseMode {
    /// Clearly labeled structural distractors — no rationale, no causal signal.
    /// Baseline: does irrelevant information dilute the context?
    #[default]
    Structural,
    /// Noise uses causal semantic class with competing rationale — harder to filter.
    /// Tests: does the agent pick up plausible-but-wrong reasoning?
    CompetingCausal,
    /// Noise has plausible rationale that contradicts the main chain.
    /// Tests: can the agent resist conflicting causal explanations?
    ConflictingMainPath,
    /// Noise offers an alternative restart point with causal justification.
    /// Tests: does the agent choose the wrong recovery node?
    CompetingRestartPoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Domain {
    Operations,
    SoftwareDebugging,
}

/// A generated node in the seed graph.
#[derive(Debug, Clone)]
pub struct GeneratedNode {
    pub node_id: String,
    pub node_kind: String,
    pub title: String,
    pub summary: String,
    pub detail: Option<String>,
    pub labels: Vec<String>,
    pub properties: BTreeMap<String, String>,
}

/// A generated relationship in the seed graph.
#[derive(Debug, Clone)]
pub struct GeneratedRelation {
    pub source_node_id: String,
    pub target_node_id: String,
    pub relation_type: String,
    pub semantic_class: RelationSemanticClass,
    pub rationale: Option<String>,
    pub motivation: Option<String>,
    pub method: Option<String>,
    pub decision_id: Option<String>,
    pub caused_by_node_id: Option<String>,
    pub sequence: Option<u32>,
}

/// Complete generated graph seed.
#[derive(Debug, Clone)]
pub struct GeneratedSeed {
    pub root: GeneratedNode,
    pub nodes: Vec<GeneratedNode>,
    pub relations: Vec<GeneratedRelation>,
    pub config: GraphSeedConfig,
}

impl GeneratedSeed {
    pub fn total_nodes(&self) -> usize {
        1 + self.nodes.len()
    }

    pub fn total_relations(&self) -> usize {
        self.relations.len()
    }

    pub fn nodes_with_detail(&self) -> usize {
        let root_detail = if self.root.detail.is_some() { 1 } else { 0 };
        root_detail + self.nodes.iter().filter(|n| n.detail.is_some()).count()
    }
}

impl GraphSeedConfig {
    pub fn micro(domain: Domain) -> Self {
        Self {
            chain_length: 3,
            noise_branches: 0,
            detail_density: 1.0,
            relation_mix: RelationMix::Explanatory,
            domain,
            id_prefix: "micro".to_string(),
            noise_mode: NoiseMode::default(),
            seed: 0,
        }
    }

    pub fn meso(domain: Domain) -> Self {
        Self {
            chain_length: 5,
            noise_branches: 3,
            detail_density: 0.6,
            relation_mix: RelationMix::Explanatory,
            domain,
            id_prefix: "meso".to_string(),
            noise_mode: NoiseMode::default(),
            seed: 0,
        }
    }

    pub fn stress(domain: Domain) -> Self {
        Self {
            chain_length: 8,
            noise_branches: 5,
            detail_density: 0.3,
            relation_mix: RelationMix::Explanatory,
            domain,
            id_prefix: "stress".to_string(),
            noise_mode: NoiseMode::default(),
            seed: 0,
        }
    }
}

/// Generate a complete graph seed from the config.
pub fn generate_seed(config: GraphSeedConfig) -> GeneratedSeed {
    let vocab = domain_vocabulary(config.domain);
    let mut nodes = Vec::new();
    let mut relations = Vec::new();

    let root = GeneratedNode {
        node_id: format!("{}:root", config.id_prefix),
        node_kind: vocab.root_kind.to_string(),
        title: format!("{} root", vocab.root_kind),
        summary: vocab.root_summary.to_string(),
        detail: if config.detail_density >= 1.0 {
            Some(vocab.root_detail.to_string())
        } else {
            None
        },
        labels: vec![vocab.root_kind.to_string()],
        properties: BTreeMap::new(),
    };

    let mut previous_id = root.node_id.clone();

    build_causal_chain(
        &config,
        &vocab,
        &mut nodes,
        &mut relations,
        &mut previous_id,
    );
    build_noise_branches(&config, &vocab, &root, &mut nodes, &mut relations);

    GeneratedSeed {
        root,
        nodes,
        relations,
        config,
    }
}

/// Build the causal chain: rotated node-kind sequences per seed.
fn build_causal_chain(
    config: &GraphSeedConfig,
    vocab: &DomainVocabulary,
    nodes: &mut Vec<GeneratedNode>,
    relations: &mut Vec<GeneratedRelation>,
    previous_id: &mut String,
) {
    let seed_offset = config.seed;
    for depth in 0..config.chain_length {
        let kind_index = (depth + seed_offset) % vocab.chain_kinds.len();
        let kind = vocab.chain_kinds[kind_index];
        let node_id = format!("{}:chain-{}", config.id_prefix, depth);

        let semantic_class = resolve_chain_semantic_class(config, vocab, kind_index, depth);
        relations.push(build_chain_relation(
            config,
            vocab,
            kind_index,
            depth,
            kind,
            semantic_class,
            previous_id,
        ));
        nodes.push(build_chain_node(
            config,
            vocab,
            kind,
            node_id.clone(),
            depth,
        ));
        *previous_id = node_id;
    }
}

fn build_chain_relation(
    config: &GraphSeedConfig,
    vocab: &DomainVocabulary,
    kind_index: usize,
    depth: usize,
    kind: &str,
    semantic_class: RelationSemanticClass,
    previous_id: &str,
) -> GeneratedRelation {
    let rationale = match config.relation_mix {
        RelationMix::Structural => None,
        _ if config.seed == 0 => Some(format!("{} at depth {depth}", vocab.chain_rationale)),
        _ => Some(format!(
            "{} at depth {depth} (variant {})",
            vocab.chain_rationale, config.seed
        )),
    };
    let method = if config.relation_mix != RelationMix::Structural {
        Some(format!("{kind} verification"))
    } else {
        None
    };

    GeneratedRelation {
        source_node_id: previous_id.to_string(),
        target_node_id: format!("{}:chain-{}", config.id_prefix, depth),
        relation_type: vocab.chain_relation_types[kind_index % vocab.chain_relation_types.len()]
            .to_string(),
        semantic_class,
        rationale,
        motivation: None,
        method,
        decision_id: Some(format!("{}:decision-{}", config.id_prefix, depth)),
        caused_by_node_id: Some(previous_id.to_string()),
        sequence: Some(depth as u32),
    }
}

fn build_chain_node(
    config: &GraphSeedConfig,
    vocab: &DomainVocabulary,
    kind: &str,
    node_id: String,
    depth: usize,
) -> GeneratedNode {
    let has_detail = config.detail_density >= 1.0
        || (depth as f64) < (config.chain_length as f64 * config.detail_density);

    GeneratedNode {
        node_id,
        node_kind: kind.to_string(),
        title: format!("{kind} {depth}"),
        summary: format!("{} step {depth}", vocab.chain_summary),
        detail: if has_detail {
            Some(format!("{} detail for step {depth}", vocab.chain_detail))
        } else {
            None
        },
        labels: vec![kind.to_string()],
        properties: BTreeMap::new(),
    }
}

fn resolve_chain_semantic_class(
    config: &GraphSeedConfig,
    vocab: &DomainVocabulary,
    kind_index: usize,
    depth: usize,
) -> RelationSemanticClass {
    match config.relation_mix {
        RelationMix::Structural => RelationSemanticClass::Structural,
        RelationMix::Explanatory => {
            vocab.chain_semantic_classes[kind_index % vocab.chain_semantic_classes.len()]
        }
        RelationMix::Mixed => {
            if depth.is_multiple_of(2) {
                vocab.chain_semantic_classes[kind_index % vocab.chain_semantic_classes.len()]
            } else {
                RelationSemanticClass::Structural
            }
        }
    }
}

/// Add noise branches per decision node.
fn build_noise_branches(
    config: &GraphSeedConfig,
    vocab: &DomainVocabulary,
    root: &GeneratedNode,
    nodes: &mut Vec<GeneratedNode>,
    relations: &mut Vec<GeneratedRelation>,
) {
    for branch in 0..config.noise_branches {
        for depth in 0..config.chain_length {
            let noise_id = format!("{}:noise-{}-{}", config.id_prefix, depth, branch);
            let source_id = if depth == 0 {
                root.node_id.clone()
            } else {
                format!("{}:chain-{}", config.id_prefix, depth - 1)
            };

            let (
                noise_semantic,
                noise_relation_type,
                noise_rationale,
                noise_motivation,
                noise_kind,
                noise_title,
                noise_summary,
            ) = build_noise_fields(config, vocab, depth, branch);
            let (noise_rationale, noise_motivation) =
                strip_noise_metadata(config.relation_mix, noise_rationale, noise_motivation);

            relations.push(GeneratedRelation {
                source_node_id: source_id,
                target_node_id: noise_id.clone(),
                relation_type: noise_relation_type,
                semantic_class: noise_semantic,
                rationale: noise_rationale,
                motivation: noise_motivation,
                method: None,
                decision_id: None,
                caused_by_node_id: None,
                sequence: Some(100 + branch as u32),
            });

            nodes.push(GeneratedNode {
                node_id: noise_id,
                node_kind: noise_kind,
                title: noise_title,
                summary: noise_summary,
                detail: None,
                labels: vec!["noise".to_string()],
                properties: BTreeMap::new(),
            });
        }
    }
}

fn strip_noise_metadata(
    relation_mix: RelationMix,
    rationale: Option<String>,
    motivation: Option<String>,
) -> (Option<String>, Option<String>) {
    if relation_mix == RelationMix::Structural {
        (None, None)
    } else {
        (rationale, motivation)
    }
}

#[allow(clippy::type_complexity)]
fn build_noise_fields(
    config: &GraphSeedConfig,
    vocab: &DomainVocabulary,
    depth: usize,
    branch: usize,
) -> (
    RelationSemanticClass,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    String,
) {
    match config.noise_mode {
        NoiseMode::Structural => (
            RelationSemanticClass::Structural,
            "DISTRACTOR".to_string(),
            None,
            None,
            "distractor".to_string(),
            format!("noise {branch} at {depth}"),
            format!("distractor branch {branch} depth {depth}"),
        ),
        NoiseMode::CompetingCausal => {
            let kind_index = (depth + branch + 1) % vocab.chain_kinds.len();
            (
                vocab.chain_semantic_classes[kind_index % vocab.chain_semantic_classes.len()],
                vocab.chain_relation_types[kind_index % vocab.chain_relation_types.len()]
                    .to_string(),
                Some(format!(
                    "alternative {} path at depth {depth} (competing branch {branch})",
                    vocab.chain_rationale
                )),
                Some("this path was considered but is not the primary causal chain".to_string()),
                vocab.chain_kinds[kind_index].to_string(),
                format!(
                    "alternative {} {depth}-{branch}",
                    vocab.chain_kinds[kind_index]
                ),
                format!(
                    "competing {} branch {branch} at depth {depth}",
                    vocab.chain_summary
                ),
            )
        }
        NoiseMode::ConflictingMainPath => {
            let kind_index = (depth + branch + 1) % vocab.chain_kinds.len();
            (
                RelationSemanticClass::Causal,
                vocab.chain_relation_types[kind_index % vocab.chain_relation_types.len()]
                    .to_string(),
                Some(format!(
                    "no {} was needed at depth {depth} — system was stable, intervention unnecessary (contradicts main chain)",
                    vocab.chain_rationale
                )),
                Some(format!(
                    "analysis shows the {} at depth {depth} was a false alarm",
                    vocab.chain_summary
                )),
                vocab.chain_kinds[kind_index].to_string(),
                format!(
                    "conflicting {} {depth}-{branch}",
                    vocab.chain_kinds[kind_index]
                ),
                format!(
                    "contradicting {} branch {branch} at depth {depth}",
                    vocab.chain_summary
                ),
            )
        }
        NoiseMode::CompetingRestartPoint => {
            let kind_index = (depth + branch + 1) % vocab.chain_kinds.len();
            (
                RelationSemanticClass::Motivational,
                "RECOVERY_CANDIDATE".to_string(),
                Some(format!(
                    "restart from this node at depth {depth} — this is a plausible recovery point with {} justification",
                    vocab.chain_rationale
                )),
                Some(format!(
                    "branch {branch} offers an alternative restart at depth {depth} with partial resolution"
                )),
                vocab.chain_kinds[kind_index].to_string(),
                format!("recovery candidate {depth}-{branch}",),
                format!(
                    "alternative restart {} branch {branch} at depth {depth}",
                    vocab.chain_summary
                ),
            )
        }
    }
}

struct DomainVocabulary {
    root_kind: &'static str,
    root_summary: &'static str,
    root_detail: &'static str,
    chain_kinds: &'static [&'static str],
    chain_relation_types: &'static [&'static str],
    chain_semantic_classes: &'static [RelationSemanticClass],
    chain_rationale: &'static str,
    chain_summary: &'static str,
    chain_detail: &'static str,
}

fn domain_vocabulary(domain: Domain) -> DomainVocabulary {
    match domain {
        Domain::Operations => DomainVocabulary {
            root_kind: "incident",
            root_summary: "System incident requiring diagnosis and recovery",
            root_detail: "Critical system event detected; causal chain under investigation",
            chain_kinds: &["decision", "task", "artifact", "evidence"],
            chain_relation_types: &["TRIGGERS", "AUTHORIZES", "PRODUCES", "VERIFIED_BY"],
            chain_semantic_classes: &[
                RelationSemanticClass::Causal,
                RelationSemanticClass::Motivational,
                RelationSemanticClass::Procedural,
                RelationSemanticClass::Evidential,
            ],
            chain_rationale: "operational response required",
            chain_summary: "operational",
            chain_detail: "operational execution",
        },
        Domain::SoftwareDebugging => DomainVocabulary {
            root_kind: "bug_report",
            root_summary: "Software defect requiring root cause analysis",
            root_detail: "User-reported defect; reproduction and fix chain under investigation",
            chain_kinds: &["hypothesis", "investigation", "fix", "test"],
            chain_relation_types: &["SUSPECTS", "INVESTIGATES", "FIXES", "VALIDATES"],
            chain_semantic_classes: &[
                RelationSemanticClass::Causal,
                RelationSemanticClass::Evidential,
                RelationSemanticClass::Procedural,
                RelationSemanticClass::Evidential,
            ],
            chain_rationale: "debugging step",
            chain_summary: "debug",
            chain_detail: "debug investigation",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn micro_generates_expected_node_count() {
        let seed = generate_seed(GraphSeedConfig::micro(Domain::Operations));
        // chain_length=3, noise=0, root=1 → 4 total
        assert_eq!(seed.total_nodes(), 4);
        assert_eq!(seed.total_relations(), 3);
        assert_eq!(seed.nodes_with_detail(), 4); // density=1.0
    }

    #[test]
    fn meso_generates_noise_branches() {
        let seed = generate_seed(GraphSeedConfig::meso(Domain::Operations));
        // chain=5, noise=3 per depth → 5 chain + 15 noise = 20 + root = 21
        assert_eq!(seed.total_nodes(), 21);
        assert!(seed.total_relations() >= 20);
    }

    #[test]
    fn stress_generates_large_graph() {
        let seed = generate_seed(GraphSeedConfig::stress(Domain::SoftwareDebugging));
        // chain=8, noise=5 per depth → 8 + 40 = 48 + root = 49
        assert_eq!(seed.total_nodes(), 49);
        assert!(seed.total_relations() > 40);
    }

    #[test]
    fn structural_mix_has_no_rationale() {
        let seed = generate_seed(GraphSeedConfig {
            chain_length: 3,
            noise_branches: 0,
            detail_density: 0.0,
            relation_mix: RelationMix::Structural,
            domain: Domain::Operations,
            id_prefix: "test".to_string(),
            noise_mode: NoiseMode::default(),
            seed: 0,
        });

        for rel in &seed.relations {
            assert_eq!(rel.semantic_class, RelationSemanticClass::Structural);
            assert!(rel.rationale.is_none());
        }
    }

    #[test]
    fn structural_mix_with_noise_has_no_rationale() {
        // Structural noise branches must have zero rationale.
        let seed = generate_seed(GraphSeedConfig {
            chain_length: 3,
            noise_branches: 2,
            detail_density: 0.0,
            relation_mix: RelationMix::Structural,
            domain: Domain::Operations,
            id_prefix: "test".to_string(),
            noise_mode: NoiseMode::Structural,
            seed: 0,
        });

        for rel in &seed.relations {
            assert!(
                rel.rationale.is_none(),
                "structural variant must have no rationale, but {:?} has {:?}",
                rel.target_node_id,
                rel.rationale
            );
        }
    }

    #[test]
    fn structural_mix_with_competing_noise_has_no_rationale() {
        // CompetingCausal noise must also be suppressed when relation_mix is Structural.
        let seed = generate_seed(GraphSeedConfig {
            chain_length: 3,
            noise_branches: 2,
            detail_density: 0.0,
            relation_mix: RelationMix::Structural,
            domain: Domain::Operations,
            id_prefix: "test".to_string(),
            noise_mode: NoiseMode::CompetingCausal,
            seed: 0,
        });

        for rel in &seed.relations {
            assert!(
                rel.rationale.is_none(),
                "structural+competing variant must have no rationale, but {:?} has {:?}",
                rel.target_node_id,
                rel.rationale
            );
            assert!(
                rel.motivation.is_none(),
                "structural+competing variant must have no motivation, but {:?} has {:?}",
                rel.target_node_id,
                rel.motivation
            );
        }
    }

    #[test]
    fn two_domains_produce_different_vocabularies() {
        let ops = generate_seed(GraphSeedConfig::micro(Domain::Operations));
        let debug = generate_seed(GraphSeedConfig::micro(Domain::SoftwareDebugging));

        assert_eq!(ops.root.node_kind, "incident");
        assert_eq!(debug.root.node_kind, "bug_report");
    }

    #[test]
    fn detail_density_controls_detail_count() {
        let full = generate_seed(GraphSeedConfig {
            detail_density: 1.0,
            ..GraphSeedConfig::micro(Domain::Operations)
        });
        let sparse = generate_seed(GraphSeedConfig {
            detail_density: 0.0,
            ..GraphSeedConfig::micro(Domain::Operations)
        });

        assert_eq!(full.nodes_with_detail(), 4);
        assert_eq!(sparse.nodes_with_detail(), 0);
    }

    #[test]
    fn competing_causal_noise_uses_domain_vocabulary() {
        let seed = generate_seed(GraphSeedConfig {
            noise_branches: 2,
            noise_mode: NoiseMode::CompetingCausal,
            ..GraphSeedConfig::meso(Domain::Operations)
        });

        let noise_relations: Vec<_> = seed
            .relations
            .iter()
            .filter(|r| r.target_node_id.contains("noise"))
            .collect();

        // Noise should use causal/motivational/etc, not just Structural
        assert!(
            noise_relations
                .iter()
                .any(|r| r.semantic_class != RelationSemanticClass::Structural),
            "competing causal noise should have non-structural semantic classes"
        );

        // Noise should have competing rationale
        assert!(
            noise_relations.iter().any(|r| r
                .rationale
                .as_deref()
                .unwrap_or("")
                .contains("alternative")),
            "competing causal noise should have alternative rationale"
        );

        // Noise should have motivation explaining it's not primary
        assert!(
            noise_relations.iter().any(|r| r.motivation.is_some()),
            "competing causal noise should have motivation"
        );
    }

    #[test]
    fn competing_causal_noise_nodes_use_domain_kinds() {
        let seed = generate_seed(GraphSeedConfig {
            noise_branches: 1,
            noise_mode: NoiseMode::CompetingCausal,
            ..GraphSeedConfig::micro(Domain::Operations)
        });

        let noise_nodes: Vec<_> = seed
            .nodes
            .iter()
            .filter(|n| n.node_id.contains("noise"))
            .collect();

        // Noise nodes should NOT be labeled "distractor" — they mimic real domain nodes
        for node in &noise_nodes {
            assert_ne!(
                node.node_kind, "distractor",
                "competing causal noise should use domain node kinds, not 'distractor'"
            );
        }
    }

    #[test]
    fn conflicting_main_path_noise_contradicts_chain() {
        let seed = generate_seed(GraphSeedConfig {
            noise_branches: 2,
            noise_mode: NoiseMode::ConflictingMainPath,
            ..GraphSeedConfig::meso(Domain::Operations)
        });

        let noise_relations: Vec<_> = seed
            .relations
            .iter()
            .filter(|r| r.target_node_id.contains("noise"))
            .collect();

        assert!(
            noise_relations
                .iter()
                .any(|r| r.semantic_class == RelationSemanticClass::Causal),
            "conflicting noise should use Causal semantic class"
        );
        assert!(
            noise_relations.iter().any(|r| r
                .rationale
                .as_deref()
                .unwrap_or("")
                .contains("contradicts main chain")),
            "conflicting noise should have contradicting rationale"
        );
        assert!(
            noise_relations.iter().any(|r| r
                .motivation
                .as_deref()
                .unwrap_or("")
                .contains("false alarm")),
            "conflicting noise should explain it contradicts"
        );
    }

    #[test]
    fn competing_restart_point_offers_recovery_candidate() {
        let seed = generate_seed(GraphSeedConfig {
            noise_branches: 2,
            noise_mode: NoiseMode::CompetingRestartPoint,
            ..GraphSeedConfig::meso(Domain::Operations)
        });

        let noise_relations: Vec<_> = seed
            .relations
            .iter()
            .filter(|r| r.target_node_id.contains("noise"))
            .collect();

        assert!(
            noise_relations
                .iter()
                .any(|r| r.semantic_class == RelationSemanticClass::Motivational),
            "competing restart noise should use Motivational semantic class"
        );
        assert!(
            noise_relations.iter().any(|r| r
                .rationale
                .as_deref()
                .unwrap_or("")
                .contains("restart from this node")),
            "competing restart noise should suggest restart"
        );
        assert!(
            noise_relations
                .iter()
                .any(|r| r.relation_type == "RECOVERY_CANDIDATE"),
            "competing restart noise should use RECOVERY_CANDIDATE relation type"
        );
    }

    #[test]
    fn structural_noise_mode_is_backward_compatible() {
        let default_seed = generate_seed(GraphSeedConfig::meso(Domain::Operations));
        let explicit_seed = generate_seed(GraphSeedConfig {
            noise_mode: NoiseMode::Structural,
            ..GraphSeedConfig::meso(Domain::Operations)
        });

        assert_eq!(default_seed.total_nodes(), explicit_seed.total_nodes());
        assert_eq!(
            default_seed.total_relations(),
            explicit_seed.total_relations()
        );
    }

    #[test]
    fn different_seeds_produce_different_graphs() {
        let seed0 = generate_seed(GraphSeedConfig {
            seed: 0,
            ..GraphSeedConfig::meso(Domain::Operations)
        });
        let seed1 = generate_seed(GraphSeedConfig {
            seed: 1,
            ..GraphSeedConfig::meso(Domain::Operations)
        });
        let seed2 = generate_seed(GraphSeedConfig {
            seed: 2,
            ..GraphSeedConfig::meso(Domain::Operations)
        });

        // Same structure size
        assert_eq!(seed0.total_nodes(), seed1.total_nodes());
        assert_eq!(seed0.total_relations(), seed2.total_relations());

        // Different node kinds at depth 0 (rotated by seed)
        let kind0 = &seed0.nodes[0].node_kind;
        let kind1 = &seed1.nodes[0].node_kind;
        assert_ne!(
            kind0, kind1,
            "seed 0 and seed 1 should produce different node kinds at depth 0: {kind0} vs {kind1}"
        );

        // Different rationale text
        let rat0 = seed0.relations[0].rationale.as_deref().unwrap_or("");
        let rat1 = seed1.relations[0].rationale.as_deref().unwrap_or("");
        assert_ne!(
            rat0, rat1,
            "seed 0 and seed 1 should produce different rationale: {rat0} vs {rat1}"
        );
    }
}
