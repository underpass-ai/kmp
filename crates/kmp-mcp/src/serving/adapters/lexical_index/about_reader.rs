use kmp_domain::{
    BundleNode, BundleNodeDetail, DimensionSelection, GraphPointReads, NodeRelationProjection,
};
use kmp_proto::v1beta1::MemoryEvidence;
use kmp_proto_mapping::v1beta1::{
    LanguageSignals, LexicalProfile, LexicalReach, LexicalReading, LexicalRow, ReachRelation,
};

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

    /// The candidate an entry's text makes, and its row.
    pub(super) fn entry_row(
        node: &BundleNode,
        profile: &LexicalProfile,
    ) -> Option<(MemoryEvidence, LexicalRow)> {
        LexicalReading::entry_candidate(node).map(|item| {
            let row = LexicalRow::read(&item, profile);
            (item, row)
        })
    }

    /// The candidate an evidence node's detail makes, and its row.
    pub(super) fn evidence_row(
        node: &BundleNode,
        detail: &BundleNodeDetail,
        supports: Vec<String>,
        profile: &LexicalProfile,
    ) -> (MemoryEvidence, LexicalRow) {
        let item = LexicalReading::evidence_candidate(node, detail, supports);
        let row = LexicalRow::read(&item, profile);
        (item, row)
    }

    /// What `item`'s graph can lend its focus beyond `row` (P14): the
    /// details of its refs and the kept relations incident to them, as
    /// `detail` and `kept` give them.
    pub(super) fn reach(
        item: &MemoryEvidence,
        row: &LexicalRow,
        profile: &LexicalProfile,
        mut detail: impl FnMut(&str) -> Result<Option<String>, String>,
        mut kept: impl FnMut(&str) -> Result<Vec<NodeRelationProjection>, String>,
    ) -> Result<LexicalReach, String> {
        let mut details = Vec::new();
        let mut relations = Vec::new();
        for reference in LexicalReach::refs(item) {
            details.extend(detail(&reference)?);
            relations.extend(kept(&reference)?);
        }
        Ok(LexicalReach::read(
            item,
            row,
            profile,
            details.iter().map(String::as_str),
            relations.iter().map(|relation| ReachRelation {
                source: &relation.source_node_id,
                target: &relation.target_node_id,
                relation_type: &relation.relation_type,
                explanation: &relation.explanation,
            }),
        ))
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
