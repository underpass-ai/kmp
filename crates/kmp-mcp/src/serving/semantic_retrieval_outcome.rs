use kmp_proto_mapping::v1beta1::SemanticCandidateRanking;

/// A stable result for one retrieval selection, including a declared fallback.
#[derive(Clone)]
pub(crate) struct SemanticRetrievalOutcome {
    pub ranking: Option<SemanticCandidateRanking>,
    pub warning: Option<String>,
}
