use std::collections::{BTreeMap, BTreeSet};

use kmp_domain::{BundleNodeDetail, KmpBundle, KmpMode, RelationSemanticClass, ResolutionTier};

use crate::queries::ContextRenderOptions;
use crate::queries::bundle_section_renderer::{render_detail, render_node, render_relationship};

/// A rendered section tagged with its resolution tier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TieredSection {
    pub tier: ResolutionTier,
    pub content: String,
}

/// Classify bundle contents into resolution tiers.
///
/// Dispatches to mode-specific classification:
/// - `ResumeFocused`: only causal spine in L1, no L2 at all
/// - All other modes: default classification with L0/L1/L2
pub(crate) fn classify_into_tiers(
    bundle: &KmpBundle,
    detail_by_node_id: &BTreeMap<&str, &BundleNodeDetail>,
    options: &ContextRenderOptions,
    resolved_mode: KmpMode,
) -> Vec<TieredSection> {
    match resolved_mode {
        KmpMode::ResumeFocused => classify_resume_focused(bundle, detail_by_node_id, options),
        _ => classify_default(bundle, detail_by_node_id, options),
    }
}

/// Default classification: L0 summary, L1 causal spine, L2 evidence pack.
fn classify_default(
    bundle: &KmpBundle,
    detail_by_node_id: &BTreeMap<&str, &BundleNodeDetail>,
    options: &ContextRenderOptions,
) -> Vec<TieredSection> {
    let max_tier = options.max_tier.unwrap_or(ResolutionTier::L2EvidencePack);
    let mut sections = Vec::new();

    // ── L0 Summary ──────────────────────────────────────────────────
    sections.push(TieredSection {
        tier: ResolutionTier::L0Summary,
        content: render_l0_summary(bundle, options),
    });

    if max_tier < ResolutionTier::L1CausalSpine {
        return sections;
    }

    // ── L1 Causal Spine ─────────────────────────────────────────────
    // Root node
    sections.push(TieredSection {
        tier: ResolutionTier::L1CausalSpine,
        content: render_node(bundle.root_node()),
    });

    // Focus node (if different from root)
    let focus_node_id = find_focus_node(bundle, options);

    if let Some(focus_node) = focus_node_id {
        sections.push(TieredSection {
            tier: ResolutionTier::L1CausalSpine,
            content: render_node(focus_node),
        });
    }

    // Explanatory relationships (causal, motivational, evidential, constraint)
    let relationships = salience_sorted_relationships(bundle);
    append_l1_explanatory(&mut sections, &relationships);

    if max_tier < ResolutionTier::L2EvidencePack {
        return sections;
    }

    // ── L2 Evidence Pack ────────────────────────────────────────────
    let focus_id = focus_node_id.map(|n| n.node_id());
    append_l2_evidence(
        &mut sections,
        &relationships,
        bundle,
        detail_by_node_id,
        options,
        focus_id,
    );

    sections
}

fn salience_sorted_relationships(bundle: &KmpBundle) -> Vec<&kmp_domain::BundleRelationship> {
    let mut relationships: Vec<_> = bundle.relationships().iter().collect();
    relationships.sort_by_key(|r| r.explanation().semantic_class().salience_rank());
    relationships
}

fn find_focus_node<'a>(
    bundle: &'a KmpBundle,
    options: &'a ContextRenderOptions,
) -> Option<&'a kmp_domain::BundleNode> {
    options.focus_node_id.as_deref().and_then(|fid| {
        if fid != bundle.root_node().node_id() {
            bundle.neighbor_nodes().iter().find(|n| n.node_id() == fid)
        } else {
            None
        }
    })
}

fn append_l1_explanatory(
    sections: &mut Vec<TieredSection>,
    relationships: &[&kmp_domain::BundleRelationship],
) {
    for rel in relationships {
        if is_explanatory(rel.explanation().semantic_class()) {
            sections.push(TieredSection {
                tier: ResolutionTier::L1CausalSpine,
                content: render_relationship(rel),
            });
        }
    }
}

/// Appends L2 evidence pack sections: non-explanatory relationships,
/// remaining neighbor nodes, and prioritized details.
fn append_l2_evidence(
    sections: &mut Vec<TieredSection>,
    relationships: &[&kmp_domain::BundleRelationship],
    bundle: &KmpBundle,
    detail_by_node_id: &BTreeMap<&str, &BundleNodeDetail>,
    options: &ContextRenderOptions,
    focus_id: Option<&str>,
) {
    for rel in relationships {
        if !is_explanatory(rel.explanation().semantic_class()) {
            sections.push(TieredSection {
                tier: ResolutionTier::L2EvidencePack,
                content: render_relationship(rel),
            });
        }
    }

    for node in bundle.neighbor_nodes() {
        if Some(node.node_id()) != focus_id {
            sections.push(TieredSection {
                tier: ResolutionTier::L2EvidencePack,
                content: render_node(node),
            });
        }
    }

    let details = prioritized_details(bundle, options.focus_node_id.as_deref());
    for detail in details {
        sections.push(TieredSection {
            tier: ResolutionTier::L2EvidencePack,
            content: render_detail(detail, detail_by_node_id),
        });
    }
}

/// Resume-focused classification: only causal spine in L1, no L2.
///
/// Prunes all distractor/noise branches and structural-only relationships.
/// Keeps only nodes that participate in explanatory relationships.
fn classify_resume_focused(
    bundle: &KmpBundle,
    _detail_by_node_id: &BTreeMap<&str, &BundleNodeDetail>,
    options: &ContextRenderOptions,
) -> Vec<TieredSection> {
    let max_tier = options.max_tier.unwrap_or(ResolutionTier::L2EvidencePack);
    let mut sections = Vec::new();

    // L0: compact summary (same as default)
    sections.push(TieredSection {
        tier: ResolutionTier::L0Summary,
        content: render_l0_summary(bundle, options),
    });

    if max_tier < ResolutionTier::L1CausalSpine {
        return sections;
    }

    // L1: ONLY the causal spine

    // Root node
    sections.push(TieredSection {
        tier: ResolutionTier::L1CausalSpine,
        content: render_node(bundle.root_node()),
    });

    // Focus node (if different from root)
    let focus_node = options.focus_node_id.as_deref().and_then(|fid| {
        if fid != bundle.root_node().node_id() {
            bundle.neighbor_nodes().iter().find(|n| n.node_id() == fid)
        } else {
            None
        }
    });
    if let Some(focus_node) = focus_node {
        sections.push(TieredSection {
            tier: ResolutionTier::L1CausalSpine,
            content: render_node(focus_node),
        });
    }

    // Collect causal-spine node IDs from explanatory relationships
    let causal_node_ids: BTreeSet<&str> = bundle
        .relationships()
        .iter()
        .filter(|r| is_explanatory(r.explanation().semantic_class()))
        .flat_map(|r| [r.source_node_id(), r.target_node_id()])
        .collect();

    // Causal-spine neighbor nodes only (not distractors)
    let focus_id = focus_node.map(|n| n.node_id());
    for node in bundle.neighbor_nodes() {
        if Some(node.node_id()) != focus_id && causal_node_ids.contains(node.node_id()) {
            sections.push(TieredSection {
                tier: ResolutionTier::L1CausalSpine,
                content: render_node(node),
            });
        }
    }

    // ALL explanatory relationships (sorted by salience)
    let relationships = salience_sorted_relationships(bundle);
    append_l1_explanatory(&mut sections, &relationships);

    // NO L2. Distractors, structural relationships, and details are dropped entirely.
    // This trades completeness for causal chain preservation under token pressure.

    sections
}

/// Compact L0 summary: objective, status, blocker, next action.
fn render_l0_summary(bundle: &KmpBundle, options: &ContextRenderOptions) -> String {
    let root = bundle.root_node();
    let objective = if root.summary().trim().is_empty() {
        root.title().to_string()
    } else {
        format!("{} — {}", root.title(), root.summary().trim())
    };

    let status = root.status();

    // Blocker: look for constraint relationships
    let blocker = bundle
        .relationships()
        .iter()
        .find(|r| r.explanation().semantic_class() == &RelationSemanticClass::Constraint)
        .and_then(|r| r.explanation().rationale())
        .unwrap_or("none identified");

    // Next action: highest-priority causal/motivational relationship
    let next_action = bundle
        .relationships()
        .iter()
        .filter(|r| {
            matches!(
                r.explanation().semantic_class(),
                RelationSemanticClass::Causal | RelationSemanticClass::Motivational
            )
        })
        .min_by_key(|r| r.explanation().semantic_class().salience_rank())
        .map(|r| {
            let target = r.target_node_id();
            let focus_label = if options.focus_node_id.as_deref() == Some(target) {
                " (focus)"
            } else {
                ""
            };
            format!("{} → {}{}", r.relationship_type(), target, focus_label)
        })
        .unwrap_or_else(|| "continue".to_string());

    format!("Objective: {objective}\nStatus: {status}\nBlocker: {blocker}\nNext: {next_action}")
}

fn is_explanatory(class: &RelationSemanticClass) -> bool {
    matches!(
        class,
        RelationSemanticClass::Causal
            | RelationSemanticClass::Motivational
            | RelationSemanticClass::Evidential
            | RelationSemanticClass::Constraint
    )
}

fn prioritized_details<'a>(
    bundle: &'a KmpBundle,
    focus_node_id: Option<&str>,
) -> Vec<&'a BundleNodeDetail> {
    let Some(focus_node_id) = focus_node_id else {
        return bundle.node_details().iter().collect();
    };
    let (focused, remaining): (Vec<_>, Vec<_>) = bundle
        .node_details()
        .iter()
        .partition(|d| d.node_id() == focus_node_id);
    focused.into_iter().chain(remaining).collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use kmp_domain::{
        BundleMetadata, BundleNode, BundleNodeDetail, BundleRelationship, CaseId, KmpBundle,
        KmpMode, RelationExplanation, RelationSemanticClass, ResolutionTier, Role,
    };

    use crate::queries::ContextRenderOptions;

    use super::classify_into_tiers;

    fn sample_bundle() -> KmpBundle {
        KmpBundle::new(
            CaseId::new("root").expect("valid"),
            Role::new("dev").expect("valid"),
            BundleNode::new(
                "root",
                "incident",
                "Incident Alpha",
                "System outage",
                "ACTIVE",
                vec![],
                BTreeMap::new(),
            ),
            vec![
                BundleNode::new(
                    "n1",
                    "decision",
                    "Decision",
                    "Recovery decision",
                    "ACTIVE",
                    vec![],
                    BTreeMap::new(),
                ),
                BundleNode::new(
                    "n2",
                    "task",
                    "Task",
                    "Execute repair",
                    "READY",
                    vec![],
                    BTreeMap::new(),
                ),
            ],
            vec![
                BundleRelationship::new(
                    "root",
                    "n1",
                    "TRIGGERS",
                    RelationExplanation::new(RelationSemanticClass::Causal)
                        .with_rationale("failure triggered recovery"),
                ),
                BundleRelationship::new(
                    "root",
                    "n2",
                    "CONTAINS",
                    RelationExplanation::new(RelationSemanticClass::Structural),
                ),
            ],
            vec![BundleNodeDetail::new("root", "Extended detail", "h1", 1)],
            BundleMetadata::initial("0.1.0"),
        )
        .expect("valid")
    }

    #[test]
    fn l0_summary_is_always_first() {
        let bundle = sample_bundle();
        let detail_map = bundle
            .node_details()
            .iter()
            .map(|d| (d.node_id(), d))
            .collect();
        let sections = classify_into_tiers(
            &bundle,
            &detail_map,
            &ContextRenderOptions::default(),
            KmpMode::ReasonPreserving,
        );

        assert_eq!(sections[0].tier, ResolutionTier::L0Summary);
        assert!(sections[0].content.contains("Objective:"));
        assert!(sections[0].content.contains("Status:"));
    }

    #[test]
    fn causal_relations_go_to_l1() {
        let bundle = sample_bundle();
        let detail_map = bundle
            .node_details()
            .iter()
            .map(|d| (d.node_id(), d))
            .collect();
        let sections = classify_into_tiers(
            &bundle,
            &detail_map,
            &ContextRenderOptions::default(),
            KmpMode::ReasonPreserving,
        );

        let l1_sections: Vec<_> = sections
            .iter()
            .filter(|s| s.tier == ResolutionTier::L1CausalSpine)
            .collect();
        assert!(l1_sections.iter().any(|s| s.content.contains("[causal]")));
        assert!(
            !l1_sections
                .iter()
                .any(|s| s.content.contains("[structural]"))
        );
    }

    #[test]
    fn structural_relations_go_to_l2() {
        let bundle = sample_bundle();
        let detail_map = bundle
            .node_details()
            .iter()
            .map(|d| (d.node_id(), d))
            .collect();
        let sections = classify_into_tiers(
            &bundle,
            &detail_map,
            &ContextRenderOptions::default(),
            KmpMode::ReasonPreserving,
        );

        let l2_sections: Vec<_> = sections
            .iter()
            .filter(|s| s.tier == ResolutionTier::L2EvidencePack)
            .collect();
        assert!(
            l2_sections
                .iter()
                .any(|s| s.content.contains("[structural]"))
        );
    }

    #[test]
    fn details_go_to_l2() {
        let bundle = sample_bundle();
        let detail_map = bundle
            .node_details()
            .iter()
            .map(|d| (d.node_id(), d))
            .collect();
        let sections = classify_into_tiers(
            &bundle,
            &detail_map,
            &ContextRenderOptions::default(),
            KmpMode::ReasonPreserving,
        );

        let l2_sections: Vec<_> = sections
            .iter()
            .filter(|s| s.tier == ResolutionTier::L2EvidencePack)
            .collect();
        assert!(l2_sections.iter().any(|s| s.content.contains("Detail")));
    }

    #[test]
    fn max_tier_l0_only_returns_summary() {
        let bundle = sample_bundle();
        let detail_map = bundle
            .node_details()
            .iter()
            .map(|d| (d.node_id(), d))
            .collect();
        let sections = classify_into_tiers(
            &bundle,
            &detail_map,
            &ContextRenderOptions {
                max_tier: Some(ResolutionTier::L0Summary),
                ..Default::default()
            },
            KmpMode::ReasonPreserving,
        );

        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].tier, ResolutionTier::L0Summary);
    }

    #[test]
    fn max_tier_l1_excludes_l2() {
        let bundle = sample_bundle();
        let detail_map = bundle
            .node_details()
            .iter()
            .map(|d| (d.node_id(), d))
            .collect();
        let sections = classify_into_tiers(
            &bundle,
            &detail_map,
            &ContextRenderOptions {
                max_tier: Some(ResolutionTier::L1CausalSpine),
                ..Default::default()
            },
            KmpMode::ReasonPreserving,
        );

        assert!(
            sections
                .iter()
                .all(|s| s.tier != ResolutionTier::L2EvidencePack)
        );
        assert!(
            sections
                .iter()
                .any(|s| s.tier == ResolutionTier::L1CausalSpine)
        );
    }

    #[test]
    fn l0_summary_identifies_blocker_and_next_action() {
        let bundle = KmpBundle::new(
            CaseId::new("root").expect("valid"),
            Role::new("dev").expect("valid"),
            BundleNode::new(
                "root",
                "incident",
                "Root",
                "Outage",
                "BLOCKED",
                vec![],
                BTreeMap::new(),
            ),
            vec![BundleNode::new(
                "n1",
                "task",
                "Fix",
                "",
                "ACTIVE",
                vec![],
                BTreeMap::new(),
            )],
            vec![
                BundleRelationship::new(
                    "root",
                    "n1",
                    "TRIGGERS",
                    RelationExplanation::new(RelationSemanticClass::Causal)
                        .with_rationale("must fix first"),
                ),
                BundleRelationship::new(
                    "root",
                    "n1",
                    "BLOCKED_BY",
                    RelationExplanation::new(RelationSemanticClass::Constraint)
                        .with_rationale("waiting for approval"),
                ),
            ],
            vec![],
            BundleMetadata::initial("0.1.0"),
        )
        .expect("valid");

        let detail_map = BTreeMap::new();
        let sections = classify_into_tiers(
            &bundle,
            &detail_map,
            &ContextRenderOptions::default(),
            KmpMode::ReasonPreserving,
        );
        let l0 = &sections[0].content;

        assert!(l0.contains("Blocker: waiting for approval"));
        assert!(l0.contains("Next: TRIGGERS"));
    }

    #[test]
    fn resume_focused_excludes_distractor_nodes() {
        let bundle = bundle_with_distractors();
        let detail_map = BTreeMap::new();
        let sections = classify_into_tiers(
            &bundle,
            &detail_map,
            &ContextRenderOptions::default(),
            KmpMode::ResumeFocused,
        );

        // No L2 sections at all
        assert!(
            sections
                .iter()
                .all(|s| s.tier != ResolutionTier::L2EvidencePack)
        );
        // No distractor content
        let all_content: String = sections.iter().map(|s| s.content.as_str()).collect();
        assert!(!all_content.contains("distractor"));
    }

    #[test]
    fn resume_focused_keeps_causal_relationships() {
        let bundle = bundle_with_distractors();
        let detail_map = BTreeMap::new();
        let sections = classify_into_tiers(
            &bundle,
            &detail_map,
            &ContextRenderOptions::default(),
            KmpMode::ResumeFocused,
        );

        let l1: Vec<_> = sections
            .iter()
            .filter(|s| s.tier == ResolutionTier::L1CausalSpine)
            .collect();
        assert!(l1.iter().any(|s| s.content.contains("[causal]")));
        assert!(!l1.iter().any(|s| s.content.contains("[structural]")));
    }

    #[test]
    fn resume_focused_includes_causal_spine_nodes() {
        let bundle = bundle_with_distractors();
        let detail_map = BTreeMap::new();
        let sections = classify_into_tiers(
            &bundle,
            &detail_map,
            &ContextRenderOptions::default(),
            KmpMode::ResumeFocused,
        );

        let l1_content: String = sections
            .iter()
            .filter(|s| s.tier == ResolutionTier::L1CausalSpine)
            .map(|s| s.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        // Causal chain node should be in L1
        assert!(
            l1_content.contains("Chain Node"),
            "causal chain node should be in L1"
        );
        // Root should be in L1
        assert!(l1_content.contains("Root"), "root should be in L1");
    }

    fn bundle_with_distractors() -> KmpBundle {
        KmpBundle::new(
            CaseId::new("root").expect("valid"),
            Role::new("dev").expect("valid"),
            BundleNode::new(
                "root",
                "incident",
                "Root",
                "Outage",
                "ACTIVE",
                vec![],
                BTreeMap::new(),
            ),
            vec![
                BundleNode::new(
                    "chain-0",
                    "decision",
                    "Chain Node",
                    "Recovery",
                    "ACTIVE",
                    vec![],
                    BTreeMap::new(),
                ),
                BundleNode::new(
                    "dist-0",
                    "distractor",
                    "distractor 0",
                    "",
                    "ACTIVE",
                    vec![],
                    BTreeMap::new(),
                ),
                BundleNode::new(
                    "dist-1",
                    "distractor",
                    "distractor 1",
                    "",
                    "ACTIVE",
                    vec![],
                    BTreeMap::new(),
                ),
                BundleNode::new(
                    "dist-2",
                    "distractor",
                    "distractor 2",
                    "",
                    "ACTIVE",
                    vec![],
                    BTreeMap::new(),
                ),
            ],
            vec![
                BundleRelationship::new(
                    "root",
                    "chain-0",
                    "TRIGGERS",
                    RelationExplanation::new(RelationSemanticClass::Causal)
                        .with_rationale("failure triggered recovery"),
                ),
                BundleRelationship::new(
                    "root",
                    "dist-0",
                    "CONTAINS",
                    RelationExplanation::new(RelationSemanticClass::Structural),
                ),
                BundleRelationship::new(
                    "root",
                    "dist-1",
                    "CONTAINS",
                    RelationExplanation::new(RelationSemanticClass::Structural),
                ),
                BundleRelationship::new(
                    "root",
                    "dist-2",
                    "CONTAINS",
                    RelationExplanation::new(RelationSemanticClass::Structural),
                ),
            ],
            vec![],
            BundleMetadata::initial("0.1.0"),
        )
        .expect("valid")
    }
}
