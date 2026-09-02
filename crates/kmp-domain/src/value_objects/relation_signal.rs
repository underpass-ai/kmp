use crate::{
    MemoryRelationQuality, MemoryRelationType, RelationExplanation, RelationSemanticClass,
};

/// How much retrieval weight a stored relation carries.
///
/// The writer already judges every relation it accepts: the relation type
/// fixes a quality, the semantic class fixes a salience, and the instance
/// either carries its why and its evidence or it does not. Until this value
/// object existed, that judgment reached the response as rendered text and
/// reached ranking as nothing at all — a `causal` edge with evidence scored
/// exactly like a `structural` one.
///
/// The weight is an integer so ordering stays exact and reproducible. It is
/// the product of the three judgments the writer makes, because an edge is
/// only worth following when all three agree, plus a small bonus for the
/// typed fields the instance actually filled in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct RelationSignal {
    weight: u32,
}

/// The strongest weight any single relation can reach, for callers that
/// normalize against a ceiling instead of against the observed maximum.
pub const MAX_RELATION_SIGNAL_WEIGHT: u32 = 76;

impl RelationSignal {
    /// Reads the signal a stored relation carries.
    ///
    /// `relation_type` is the wire type as stored; unknown extensions have no
    /// writer specification and are treated the way the writer treats them
    /// outside strict mode — as suspect, and therefore carrying nothing.
    pub fn read(relation_type: &str, explanation: &RelationExplanation) -> Self {
        let quality = relation_quality(relation_type, explanation);
        if quality == MemoryRelationQuality::Suspect {
            return Self { weight: 0 };
        }

        let class = class_weight(explanation.semantic_class());
        let quality = quality_weight(quality);
        let confidence = confidence_weight(explanation.confidence());
        Self {
            weight: class * quality * confidence + completeness_bonus(explanation),
        }
    }

    /// A relation that carries nothing: unknown vocabulary, a suspect
    /// downgrade, or a purely structural edge with no declared proof.
    pub fn silent() -> Self {
        Self { weight: 0 }
    }

    pub fn weight(&self) -> u32 {
        self.weight
    }

    pub fn is_silent(&self) -> bool {
        self.weight == 0
    }

    /// Whether this relation is strong enough to carry retrieval across it.
    ///
    /// Following an edge can surface a memory that shares no vocabulary with
    /// the question, which is the point; the floor is what keeps that from
    /// also surfacing everything a high-degree node happens to touch.
    pub fn carries_retrieval(&self) -> bool {
        self.weight >= RETRIEVAL_FLOOR
    }
}

/// A causal edge with evidence and declared confidence clears this; an
/// anemic succession edge does not.
const RETRIEVAL_FLOOR: u32 = 24;

/// Recomputes at read time the judgment the writer made at write time.
///
/// `kmp_write_memory` downgrades a rich relation to suspect when its proof is
/// incomplete or its confidence is low. That downgrade is reported to the
/// writer and never stored on the edge, so the reader reconstructs it from
/// the same inputs, minus the prior-context check that only the write path
/// can perform.
fn relation_quality(
    relation_type: &str,
    explanation: &RelationExplanation,
) -> MemoryRelationQuality {
    let Some(spec) = MemoryRelationType::new(relation_type)
        .ok()
        .and_then(|relation_type| relation_type.writer_spec())
    else {
        return MemoryRelationQuality::Suspect;
    };

    if spec.quality() != MemoryRelationQuality::Rich {
        return spec.quality();
    }
    if !proof_complete(explanation) || matches!(explanation.confidence(), Some("low" | "unknown")) {
        return MemoryRelationQuality::Suspect;
    }
    MemoryRelationQuality::Rich
}

fn proof_complete(explanation: &RelationExplanation) -> bool {
    let has_why = explanation.rationale().is_some() || explanation.motivation().is_some();
    has_why && explanation.evidence().is_some()
}

/// Inverts the packing salience so a higher number means a stronger edge:
/// causal 6 down to structural 1.
fn class_weight(class: &RelationSemanticClass) -> u32 {
    u32::from(6 - class.salience_rank())
}

fn quality_weight(quality: MemoryRelationQuality) -> u32 {
    match quality {
        MemoryRelationQuality::Rich => 4,
        MemoryRelationQuality::Anemic => 2,
        MemoryRelationQuality::Structural => 1,
        MemoryRelationQuality::Suspect => 0,
    }
}

fn confidence_weight(confidence: Option<&str>) -> u32 {
    match confidence {
        Some("high") => 3,
        Some("low") | Some("unknown") => 1,
        // An undeclared confidence is neither a claim of strength nor a
        // reason to discount what the writer proved with evidence.
        _ => 2,
    }
}

/// One point per typed field the instance actually filled in. It breaks ties
/// between two edges of the same class and quality in favour of the one that
/// carries more of its own proof.
fn completeness_bonus(explanation: &RelationExplanation) -> u32 {
    u32::from(explanation.rationale().is_some())
        + u32::from(explanation.motivation().is_some())
        + u32::from(explanation.evidence().is_some())
        + u32::from(explanation.caused_by_node_id().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rich_causal() -> RelationExplanation {
        RelationExplanation::new(RelationSemanticClass::Causal)
            .with_rationale("the migration required a shared-writer engine")
            .with_evidence("recorded in the architecture decision record")
            .with_confidence("high")
    }

    #[test]
    fn a_proven_causal_relation_outweighs_a_bare_structural_one() {
        let causal = RelationSignal::read("depends_on", &rich_causal());
        let structural = RelationSignal::read(
            "contains_entry",
            &RelationExplanation::new(RelationSemanticClass::Structural),
        );

        assert!(causal.weight() > structural.weight());
        assert!(causal.carries_retrieval());
        assert!(!structural.carries_retrieval());
    }

    #[test]
    fn a_rich_relation_without_its_proof_is_downgraded_the_way_the_writer_downgrades_it() {
        let unproven = RelationSignal::read(
            "depends_on",
            &RelationExplanation::new(RelationSemanticClass::Causal)
                .with_rationale("no evidence accompanies this why"),
        );

        assert!(unproven.is_silent());
        assert!(!unproven.carries_retrieval());
    }

    #[test]
    fn low_confidence_silences_a_rich_relation() {
        let hedged = RelationSignal::read("depends_on", &rich_causal().with_confidence("low"));

        assert!(hedged.is_silent());
    }

    #[test]
    fn vocabulary_outside_the_writer_specification_carries_nothing() {
        let invented = RelationSignal::read("smells_like", &rich_causal());

        assert!(invented.is_silent());
    }

    #[test]
    fn an_anemic_relation_is_weaker_than_a_rich_one_of_the_same_class() {
        let rich = RelationSignal::read("triggers", &rich_causal());
        let anemic = RelationSignal::read(
            "follows",
            &RelationExplanation::new(RelationSemanticClass::Procedural)
                .with_rationale("the gate ran after the build")
                .with_evidence("workflow log"),
        );

        assert!(rich.weight() > anemic.weight());
    }

    #[test]
    fn a_declared_causal_parent_breaks_the_tie_between_equal_edges() {
        let plain = RelationSignal::read("triggers", &rich_causal());
        let with_parent = RelationSignal::read(
            "triggers",
            &rich_causal().with_caused_by_node_id("claim:root"),
        );

        assert!(with_parent.weight() > plain.weight());
    }

    #[test]
    fn no_relation_exceeds_the_published_ceiling() {
        let strongest = RelationSignal::read(
            "triggers",
            &rich_causal()
                .with_motivation("and the reserve had to be diverted")
                .with_caused_by_node_id("claim:root"),
        );

        assert_eq!(strongest.weight(), MAX_RELATION_SIGNAL_WEIGHT);
        assert!(RelationSignal::silent().is_silent());
    }
}
