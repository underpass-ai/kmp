use serde::{Deserialize, Serialize};

use crate::{DomainError, RelationSemanticClass};

const STRUCTURAL_CLASSES: &[RelationSemanticClass] = &[RelationSemanticClass::Structural];
const CAUSAL_CLASSES: &[RelationSemanticClass] = &[RelationSemanticClass::Causal];
const MOTIVATIONAL_OR_CAUSAL_CLASSES: &[RelationSemanticClass] = &[
    RelationSemanticClass::Causal,
    RelationSemanticClass::Motivational,
];
const PROCEDURAL_CLASSES: &[RelationSemanticClass] = &[RelationSemanticClass::Procedural];
const EVIDENTIAL_CLASSES: &[RelationSemanticClass] = &[RelationSemanticClass::Evidential];
const EVIDENTIAL_OR_MOTIVATIONAL_CLASSES: &[RelationSemanticClass] = &[
    RelationSemanticClass::Evidential,
    RelationSemanticClass::Motivational,
];
const CONSTRAINT_CLASSES: &[RelationSemanticClass] = &[RelationSemanticClass::Constraint];
const WRITER_RELATION_TYPES: &[KnownMemoryRelationType] = &[
    KnownMemoryRelationType::Follows,
    KnownMemoryRelationType::Answers,
    KnownMemoryRelationType::UsesBackground,
    KnownMemoryRelationType::DependsOn,
    KnownMemoryRelationType::ChosenBecause,
    KnownMemoryRelationType::Triggers,
    KnownMemoryRelationType::Authorizes,
    KnownMemoryRelationType::VerifiedBy,
    KnownMemoryRelationType::SemanticDeltaFrom,
    KnownMemoryRelationType::UpdatesState,
    KnownMemoryRelationType::Supports,
    KnownMemoryRelationType::Supersedes,
    KnownMemoryRelationType::Contradicts,
    KnownMemoryRelationType::SatisfiesConstraint,
    KnownMemoryRelationType::ViolatesConstraint,
    KnownMemoryRelationType::ContributesTo,
    KnownMemoryRelationType::ExcludedFrom,
    KnownMemoryRelationType::CheckedAgainst,
    KnownMemoryRelationType::DerivedFrom,
    KnownMemoryRelationType::ConfirmsSelection,
    KnownMemoryRelationType::Restates,
    KnownMemoryRelationType::Corrects,
    KnownMemoryRelationType::ComponentOf,
    KnownMemoryRelationType::TotalOf,
    KnownMemoryRelationType::SameEventAs,
    KnownMemoryRelationType::SameEntityAs,
    KnownMemoryRelationType::QualifiesAs,
    KnownMemoryRelationType::MatchesRequirement,
    KnownMemoryRelationType::Contains,
    KnownMemoryRelationType::MemberOf,
    KnownMemoryRelationType::ScopedTo,
];

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MemoryRelationType {
    canonical: String,
    known: Option<KnownMemoryRelationType>,
}

impl MemoryRelationType {
    pub fn new(value: impl AsRef<str>) -> Result<Self, DomainError> {
        let raw = value.as_ref();
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyValue("relation_type"));
        }
        let known = KnownMemoryRelationType::parse(trimmed);
        let canonical = known
            .map(|known| known.as_str().to_string())
            .unwrap_or_else(|| trimmed.to_string());
        Ok(Self { canonical, known })
    }

    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    pub fn known(&self) -> Option<KnownMemoryRelationType> {
        self.known
    }

    pub fn is_known(&self) -> bool {
        self.known.is_some()
    }

    pub fn is_structural(&self) -> bool {
        self.known
            .is_some_and(KnownMemoryRelationType::is_structural)
    }

    pub fn is_support_only(&self) -> bool {
        self.known
            .is_some_and(KnownMemoryRelationType::is_support_only)
    }

    pub fn is_operand_modeling(&self) -> bool {
        self.known
            .is_some_and(KnownMemoryRelationType::is_operand_modeling)
    }

    /// Whether this is one of the two relations that may cross an about:
    /// `same_event_as` and `same_entity_as`, declared by a writer from a
    /// `kmp_relate` proposal and nothing else.
    pub fn may_cross_abouts(&self) -> bool {
        self.known
            .is_some_and(KnownMemoryRelationType::may_cross_abouts)
    }

    pub fn is_conflict(&self) -> bool {
        self.known.is_some_and(KnownMemoryRelationType::is_conflict)
    }

    pub fn writer_spec(&self) -> Option<MemoryRelationSpec> {
        self.known.and_then(KnownMemoryRelationType::writer_spec)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnownMemoryRelationType {
    Contains,
    ContainsEntry,
    HasDimension,
    HasEvidence,
    MemberOf,
    Records,
    ScopedTo,
    Follows,
    Answers,
    UsesBackground,
    DependsOn,
    ChosenBecause,
    Triggers,
    Authorizes,
    VerifiedBy,
    SemanticDeltaFrom,
    UpdatesState,
    Supports,
    SupportsAnswer,
    Supersedes,
    Contradicts,
    ConfirmsSelection,
    SatisfiesConstraint,
    ViolatesConstraint,
    ContributesTo,
    ExcludedFrom,
    CheckedAgainst,
    DerivedFrom,
    Restates,
    Corrects,
    ComponentOf,
    TotalOf,
    SameEventAs,
    SameEntityAs,
    QualifiesAs,
    MatchesRequirement,
}

impl KnownMemoryRelationType {
    pub fn parse(value: &str) -> Option<Self> {
        match normalize_relation_type(value).as_str() {
            "contains" => Some(Self::Contains),
            "contains_entry" => Some(Self::ContainsEntry),
            "has_dimension" => Some(Self::HasDimension),
            "has_evidence" => Some(Self::HasEvidence),
            "member_of" => Some(Self::MemberOf),
            "records" => Some(Self::Records),
            "scoped_to" => Some(Self::ScopedTo),
            "follows" => Some(Self::Follows),
            "answers" => Some(Self::Answers),
            "uses_background" => Some(Self::UsesBackground),
            "depends_on" => Some(Self::DependsOn),
            "chosen_because" => Some(Self::ChosenBecause),
            "triggers" => Some(Self::Triggers),
            "authorizes" => Some(Self::Authorizes),
            "verified_by" => Some(Self::VerifiedBy),
            "semantic_delta_from" => Some(Self::SemanticDeltaFrom),
            "updates_state" => Some(Self::UpdatesState),
            "supports" => Some(Self::Supports),
            "supports_answer" => Some(Self::SupportsAnswer),
            "supersedes" => Some(Self::Supersedes),
            "contradicts" | "conflicts" | "conflicts_with" | "conflict_with" => {
                Some(Self::Contradicts)
            }
            "confirms_selection" => Some(Self::ConfirmsSelection),
            "satisfies_constraint" => Some(Self::SatisfiesConstraint),
            "violates_constraint" => Some(Self::ViolatesConstraint),
            "contributes_to" => Some(Self::ContributesTo),
            "excluded_from" => Some(Self::ExcludedFrom),
            "checked_against" => Some(Self::CheckedAgainst),
            "derived_from" => Some(Self::DerivedFrom),
            "restates" => Some(Self::Restates),
            "corrects" => Some(Self::Corrects),
            "component_of" => Some(Self::ComponentOf),
            "total_of" => Some(Self::TotalOf),
            "same_event_as" => Some(Self::SameEventAs),
            "same_entity_as" => Some(Self::SameEntityAs),
            "qualifies_as" => Some(Self::QualifiesAs),
            "matches_requirement" | "matches_question_item" => Some(Self::MatchesRequirement),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::ContainsEntry => "contains_entry",
            Self::HasDimension => "has_dimension",
            Self::HasEvidence => "has_evidence",
            Self::MemberOf => "member_of",
            Self::Records => "records",
            Self::ScopedTo => "scoped_to",
            Self::Follows => "follows",
            Self::Answers => "answers",
            Self::UsesBackground => "uses_background",
            Self::DependsOn => "depends_on",
            Self::ChosenBecause => "chosen_because",
            Self::Triggers => "triggers",
            Self::Authorizes => "authorizes",
            Self::VerifiedBy => "verified_by",
            Self::SemanticDeltaFrom => "semantic_delta_from",
            Self::UpdatesState => "updates_state",
            Self::Supports => "supports",
            Self::SupportsAnswer => "supports_answer",
            Self::Supersedes => "supersedes",
            Self::Contradicts => "contradicts",
            Self::ConfirmsSelection => "confirms_selection",
            Self::SatisfiesConstraint => "satisfies_constraint",
            Self::ViolatesConstraint => "violates_constraint",
            Self::ContributesTo => "contributes_to",
            Self::ExcludedFrom => "excluded_from",
            Self::CheckedAgainst => "checked_against",
            Self::DerivedFrom => "derived_from",
            Self::Restates => "restates",
            Self::Corrects => "corrects",
            Self::ComponentOf => "component_of",
            Self::TotalOf => "total_of",
            Self::SameEventAs => "same_event_as",
            Self::SameEntityAs => "same_entity_as",
            Self::QualifiesAs => "qualifies_as",
            Self::MatchesRequirement => "matches_requirement",
        }
    }

    pub fn is_structural(self) -> bool {
        matches!(
            self,
            Self::Contains
                | Self::ContainsEntry
                | Self::HasDimension
                | Self::HasEvidence
                | Self::MemberOf
                | Self::Records
                | Self::ScopedTo
        )
    }

    pub fn is_support_only(self) -> bool {
        matches!(self, Self::Supports | Self::SupportsAnswer)
    }

    /// The two equivalences a writer may declare across abouts.
    pub fn may_cross_abouts(self) -> bool {
        matches!(self, Self::SameEventAs | Self::SameEntityAs)
    }

    pub fn is_operand_modeling(self) -> bool {
        matches!(
            self,
            Self::ContributesTo
                | Self::ExcludedFrom
                | Self::CheckedAgainst
                | Self::DerivedFrom
                | Self::Restates
                | Self::Corrects
                | Self::ComponentOf
                | Self::TotalOf
                | Self::SameEventAs
                | Self::SameEntityAs
                | Self::QualifiesAs
                | Self::MatchesRequirement
        )
    }

    pub fn is_conflict(self) -> bool {
        matches!(self, Self::Contradicts)
    }

    pub fn writer_spec(self) -> Option<MemoryRelationSpec> {
        match self {
            Self::Follows => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Anemic,
                PROCEDURAL_CLASSES,
                "writer proved process succession but not a richer semantic dependency",
            )),
            Self::Answers => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Anemic,
                EVIDENTIAL_CLASSES,
                "writer proved answerhood but not a richer semantic dependency",
            )),
            Self::UsesBackground => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Anemic,
                EVIDENTIAL_CLASSES,
                "writer scoped the node to background without claiming causal semantics",
            )),
            Self::DependsOn => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Rich,
                CAUSAL_CLASSES,
                "explicit dependency relation with target ref, why, and evidence",
            )),
            Self::ChosenBecause => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Rich,
                MOTIVATIONAL_OR_CAUSAL_CLASSES,
                "decision relation explains why a prior memory led to the current choice",
            )),
            Self::Triggers => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Rich,
                CAUSAL_CLASSES,
                "causal relation identifies the event or decision that triggered an action",
            )),
            Self::Authorizes => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Rich,
                MOTIVATIONAL_OR_CAUSAL_CLASSES,
                "authorization relation identifies the decision that permitted an action",
            )),
            Self::VerifiedBy => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Rich,
                EVIDENTIAL_CLASSES,
                "verification relation identifies the evidence or check that verified a claim",
            )),
            Self::SemanticDeltaFrom => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Rich,
                CAUSAL_CLASSES,
                "delta relation explains how current state changes from prior memory",
            )),
            Self::UpdatesState => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Rich,
                CAUSAL_CLASSES,
                "state transition relation identifies what memory is being changed",
            )),
            Self::Supersedes => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Rich,
                EVIDENTIAL_CLASSES,
                "replacement relation identifies the superseded memory and evidence",
            )),
            Self::Contradicts => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Rich,
                EVIDENTIAL_CLASSES,
                "conflict relation identifies the contradicted memory and evidence",
            )),
            Self::SatisfiesConstraint => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Rich,
                CONSTRAINT_CLASSES,
                "constraint relation identifies the rule satisfied by the current memory",
            )),
            Self::ViolatesConstraint => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Rich,
                CONSTRAINT_CLASSES,
                "constraint relation identifies the rule violated by the current memory",
            )),
            Self::ContributesTo => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Rich,
                EVIDENTIAL_CLASSES,
                "operand relation marks a value as intentionally included in a derived result",
            )),
            Self::ExcludedFrom => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Rich,
                CONSTRAINT_CLASSES,
                "operand relation marks a value as intentionally excluded",
            )),
            Self::CheckedAgainst => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Rich,
                CONSTRAINT_CLASSES,
                "verification relation marks a value checked against a rule or window",
            )),
            Self::DerivedFrom => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Rich,
                EVIDENTIAL_CLASSES,
                "derived value relation identifies source operands or evidence",
            )),
            Self::Supports => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Rich,
                EVIDENTIAL_CLASSES,
                "evidence relation identifies the memory supported by the current observation",
            )),
            Self::ConfirmsSelection => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Rich,
                EVIDENTIAL_OR_MOTIVATIONAL_CLASSES,
                "feedback relation identifies the selection confirmed by later evidence",
            )),
            Self::Restates => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Rich,
                EVIDENTIAL_CLASSES,
                "operand relation marks a repeated claim or value as the same fact",
            )),
            Self::Corrects => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Rich,
                EVIDENTIAL_CLASSES,
                "operand relation marks a later value as correcting an earlier fact",
            )),
            Self::ComponentOf => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Rich,
                EVIDENTIAL_CLASSES,
                "operand relation marks a value as part of a larger total or set",
            )),
            Self::TotalOf => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Rich,
                EVIDENTIAL_CLASSES,
                "operand relation marks a value as an aggregate total for component values",
            )),
            Self::SameEventAs | Self::SameEntityAs => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Rich,
                EVIDENTIAL_CLASSES,
                "identity relation declares two refs the same event or entity; the one relation that may cross an about, declared by a writer from a kmp_relate proposal",
            )),
            Self::QualifiesAs => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Rich,
                EVIDENTIAL_CLASSES,
                "identity relation disambiguates whether refs describe the same item",
            )),
            Self::MatchesRequirement => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Rich,
                CONSTRAINT_CLASSES,
                "operand relation marks a ref as satisfying the query or requirement predicate",
            )),
            Self::Contains | Self::MemberOf | Self::ScopedTo => Some(MemoryRelationSpec::new(
                self,
                MemoryRelationQuality::Structural,
                STRUCTURAL_CLASSES,
                "structural relation is accepted but excluded from semantic writer quality",
            )),
            Self::ContainsEntry
            | Self::HasDimension
            | Self::HasEvidence
            | Self::Records
            | Self::SupportsAnswer => None,
        }
    }

    pub fn writer_relation_types() -> &'static [Self] {
        WRITER_RELATION_TYPES
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRelationQuality {
    Rich,
    Anemic,
    Structural,
    Suspect,
}

impl MemoryRelationQuality {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rich => "rich",
            Self::Anemic => "anemic",
            Self::Structural => "structural",
            Self::Suspect => "suspect",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryRelationSpec {
    relation_type: KnownMemoryRelationType,
    quality: MemoryRelationQuality,
    allowed_classes: &'static [RelationSemanticClass],
    reason: &'static str,
}

impl MemoryRelationSpec {
    const fn new(
        relation_type: KnownMemoryRelationType,
        quality: MemoryRelationQuality,
        allowed_classes: &'static [RelationSemanticClass],
        reason: &'static str,
    ) -> Self {
        Self {
            relation_type,
            quality,
            allowed_classes,
            reason,
        }
    }

    pub fn relation_type(&self) -> KnownMemoryRelationType {
        self.relation_type
    }

    pub fn quality(&self) -> MemoryRelationQuality {
        self.quality
    }

    pub fn allowed_classes(&self) -> &'static [RelationSemanticClass] {
        self.allowed_classes
    }

    pub fn allows_class(&self, semantic_class: &RelationSemanticClass) -> bool {
        self.allowed_classes.contains(semantic_class)
    }

    pub fn reason(&self) -> &'static str {
        self.reason
    }
}

fn normalize_relation_type(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|ch| {
            if ch == '-' || ch == ' ' {
                '_'
            } else {
                ch.to_ascii_lowercase()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::{
        KnownMemoryRelationType, MemoryRelationQuality, MemoryRelationType, RelationSemanticClass,
    };

    #[test]
    fn relation_type_canonicalizes_known_wire_names() {
        let relation = MemoryRelationType::new(" CONFLICTS-WITH ").expect("valid relation");

        assert_eq!(relation.as_str(), "contradicts");
        assert_eq!(relation.known(), Some(KnownMemoryRelationType::Contradicts));
        assert!(relation.is_conflict());
    }

    #[test]
    fn research_relation_examples_are_known_wire_types() {
        for relation in ["triggers", "authorizes", "verified_by"] {
            let relation_type = MemoryRelationType::new(relation).expect("valid relation");
            assert!(
                relation_type.is_known(),
                "{relation} should be kernel-owned"
            );
            assert_eq!(relation_type.as_str(), relation);
            assert!(relation_type.writer_spec().is_some());
        }
    }

    #[test]
    fn relation_type_preserves_unknown_extensions() {
        let relation = MemoryRelationType::new("ORG_CUSTOM_REL").expect("valid relation");

        assert_eq!(relation.as_str(), "ORG_CUSTOM_REL");
        assert_eq!(relation.known(), None);
    }

    #[test]
    fn relation_type_classifies_structural_support_and_operand_relations() {
        assert!(
            MemoryRelationType::new("contains_entry")
                .expect("valid relation")
                .is_structural()
        );
        assert!(
            MemoryRelationType::new("supports_answer")
                .expect("valid relation")
                .is_support_only()
        );
        assert!(
            MemoryRelationType::new("contributes_to")
                .expect("valid relation")
                .is_operand_modeling()
        );
        assert!(
            MemoryRelationType::new("matches_question_item")
                .expect("valid relation")
                .is_operand_modeling()
        );
    }

    #[test]
    fn writer_specs_are_kernel_owned() {
        let spec = MemoryRelationType::new("contributes_to")
            .expect("valid relation")
            .writer_spec()
            .expect("writer relation");

        assert_eq!(spec.quality(), MemoryRelationQuality::Rich);
        assert!(spec.allows_class(&RelationSemanticClass::Evidential));
        assert!(!spec.allows_class(&RelationSemanticClass::Causal));
    }

    #[test]
    fn writer_relation_type_list_contains_only_relations_with_specs() {
        for relation_type in KnownMemoryRelationType::writer_relation_types() {
            assert!(
                relation_type.writer_spec().is_some(),
                "{} must have a writer spec",
                relation_type.as_str()
            );
        }
    }
}
