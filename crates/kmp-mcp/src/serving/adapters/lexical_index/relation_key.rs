use kmp_domain::NodeRelationProjection;

/// One edge of the graph by its identity: source, target and type.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct RelationKey {
    pub(super) source: String,
    pub(super) target: String,
    pub(super) relation_type: String,
}

impl RelationKey {
    pub(super) fn new(source: &str, target: &str, relation_type: &str) -> Self {
        Self {
            source: source.to_string(),
            target: target.to_string(),
            relation_type: relation_type.to_string(),
        }
    }

    pub(super) fn of(edge: &NodeRelationProjection) -> Self {
        Self::new(
            &edge.source_node_id,
            &edge.target_node_id,
            &edge.relation_type,
        )
    }

    pub(super) fn is_contains_entry(&self) -> bool {
        self.relation_type == CONTAINS_ENTRY
    }

    pub(super) fn is_supports(&self) -> bool {
        self.relation_type == SUPPORTS
    }
}

/// The edge a label keeps an entry with; the ask's selection is made of them.
pub(super) const CONTAINS_ENTRY: &str = "contains_entry";

/// The edge from an about to each entry it records: how many entries it holds.
pub(super) const RECORDS: &str = "records";
/// The edge evidence names the entries it supports with.
pub(super) const SUPPORTS: &str = "supports";
