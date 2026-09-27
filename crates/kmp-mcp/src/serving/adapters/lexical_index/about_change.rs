use kmp_proto_mapping::v1beta1::LanguageSignals;

use super::node_state::NodeState;
use super::relation_key::RelationKey;

/// What one pass of the sidecar's maintenance decided for one about, before
/// anything is written: the node states, kept relations and candidate rows
/// that changed (`None` takes one out), and the about's language totals
/// afterwards. A `rebuilt` change replaces everything the sidecar held for
/// the about; otherwise only what it names moves, and a row equal to the one
/// held is left alone, so applying a change twice moves nothing.
#[derive(Debug, Clone, Default)]
pub(super) struct AboutChange {
    pub(super) about: String,
    pub(super) rebuilt: bool,
    pub(super) nodes: Vec<(String, Option<NodeState>)>,
    pub(super) relations: Vec<(RelationKey, Option<LanguageSignals>)>,
    /// Candidate rows, encoded (`LexicalRow::encode`): a build holds one per
    /// candidate of the about, and encoded they take a fraction of the room.
    pub(super) rows: Vec<(String, Option<Vec<u8>>)>,
    /// Nodes one hop past the ask's depth that came (`true`) or went.
    pub(super) far: Vec<(String, bool)>,
    pub(super) signals: LanguageSignals,
    pub(super) summaries: u64,
    /// The language the rows were read in.
    pub(super) language: Option<String>,
}
