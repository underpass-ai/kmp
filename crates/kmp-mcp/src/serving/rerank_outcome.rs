use kmp_proto_mapping::v1beta1::RerankCandidateRanking;

/// One frozen reranking for a selection, including a declared fallback, so
/// every page of an Ask reads the same order.
#[derive(Clone)]
pub(crate) struct RerankOutcome {
    pub ranking: Option<RerankCandidateRanking>,
    pub warning: Option<String>,
}
