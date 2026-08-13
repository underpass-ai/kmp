use crate::value_objects::Provenance;
use crate::{NodeRelationProjection, RelationExplanation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleRelationship {
    source_node_id: String,
    target_node_id: String,
    relationship_type: String,
    explanation: RelationExplanation,
    provenance: Option<Provenance>,
}

impl BundleRelationship {
    pub fn new(
        source_node_id: impl Into<String>,
        target_node_id: impl Into<String>,
        relationship_type: impl Into<String>,
        explanation: RelationExplanation,
    ) -> Self {
        Self {
            source_node_id: source_node_id.into(),
            target_node_id: target_node_id.into(),
            relationship_type: relationship_type.into(),
            explanation,
            provenance: None,
        }
    }

    pub fn with_provenance(mut self, provenance: Provenance) -> Self {
        self.provenance = Some(provenance);
        self
    }

    pub fn from_projection(relationship: &NodeRelationProjection) -> Self {
        Self {
            source_node_id: relationship.source_node_id.clone(),
            target_node_id: relationship.target_node_id.clone(),
            relationship_type: relationship.relation_type.clone(),
            explanation: relationship.explanation.clone(),
            provenance: None,
        }
    }

    pub fn source_node_id(&self) -> &str {
        &self.source_node_id
    }

    pub fn target_node_id(&self) -> &str {
        &self.target_node_id
    }

    pub fn relationship_type(&self) -> &str {
        &self.relationship_type
    }

    pub fn explanation(&self) -> &RelationExplanation {
        &self.explanation
    }

    pub fn provenance(&self) -> Option<&Provenance> {
        self.provenance.as_ref()
    }
}
