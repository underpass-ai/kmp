use kmp_domain::{
    BundleNode, BundleNodeDetail, DimensionSelection, GraphPointReads, NodeRelationProjection,
};
use kmp_proto_mapping::v1beta1::{LanguageSignals, LexicalProfile, LexicalReading, LexicalRow};

use super::relation_key::RelationKey;

/// Reads one about's nodes, bodies and edges from a snapshot the way an ask
/// of the about reads them: the same selection of `contains_entry` edges,
/// the same candidates, the same terms.
pub(super) struct AboutReader<'r> {
    reads: &'r dyn GraphPointReads,
    about: &'r str,
    selection: DimensionSelection,
}

fn port(error: kmp_domain::PortError) -> String {
    format!("lexical index: {error}")
}

impl<'r> AboutReader<'r> {
    pub(super) fn new(reads: &'r dyn GraphPointReads, about: &'r str) -> Self {
        Self {
            reads,
            about,
            // What an ask with no dimensions selects: the about's own labels.
            selection: DimensionSelection::default().resolve_current_about(about),
        }
    }

    pub(super) fn about(&self) -> &str {
        self.about
    }

    pub(super) fn node(&self, id: &str) -> Result<Option<BundleNode>, String> {
        Ok(self
            .reads
            .node(id)
            .map_err(port)?
            .map(|node| BundleNode::from_projection(&node)))
    }

    pub(super) fn detail(&self, id: &str) -> Result<Option<BundleNodeDetail>, String> {
        Ok(self
            .reads
            .detail(id)
            .map_err(port)?
            .map(|detail| BundleNodeDetail::from_projection(&detail)))
    }

    pub(super) fn relation(
        &self,
        key: &RelationKey,
    ) -> Result<Option<NodeRelationProjection>, String> {
        self.reads
            .relation(&key.source, &key.target, &key.relation_type)
            .map_err(port)
    }

    pub(super) fn outgoing(
        &self,
        id: &str,
        relation_type: Option<&str>,
    ) -> Result<Vec<NodeRelationProjection>, String> {
        self.reads.outgoing(id, relation_type).map_err(port)
    }

    pub(super) fn incoming(
        &self,
        id: &str,
        relation_type: Option<&str>,
    ) -> Result<Vec<NodeRelationProjection>, String> {
        self.reads.incoming(id, relation_type).map_err(port)
    }

    /// Whether the ask's selection keeps a `contains_entry` edge whose two
    /// ends it reaches: its coordinate must be one of the about's own.
    pub(super) fn keeps_contains_entry(&self, edge: &NodeRelationProjection) -> bool {
        let explanation = &edge.explanation;
        self.selection.includes_coordinate(
            explanation.dimension().unwrap_or_default(),
            explanation.scope_id().unwrap_or_default(),
        )
    }

    /// The candidate an entry's text makes, as a row.
    pub(super) fn entry_row(node: &BundleNode, profile: &LexicalProfile) -> Option<LexicalRow> {
        LexicalReading::entry_candidate(node).map(|item| LexicalRow::read(&item, profile))
    }

    /// The candidate an evidence node's detail makes, as a row.
    pub(super) fn evidence_row(
        node: &BundleNode,
        detail: &BundleNodeDetail,
        supports: Vec<String>,
        profile: &LexicalProfile,
    ) -> LexicalRow {
        LexicalRow::read(
            &LexicalReading::evidence_candidate(node, detail, supports),
            profile,
        )
    }

    /// What a kept node adds to its about's language.
    pub(super) fn node_unit(
        node: &BundleNode,
        detail: Option<&BundleNodeDetail>,
    ) -> (LanguageSignals, bool) {
        (
            LexicalReading::node_signals(node, detail),
            LexicalReading::carries_search_summary(node),
        )
    }
}
