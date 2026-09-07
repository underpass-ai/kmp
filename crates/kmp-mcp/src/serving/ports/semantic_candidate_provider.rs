use std::{future::Future, pin::Pin};

use crate::serving::semantic_retrieval_outcome::SemanticRetrievalOutcome;
use kmp_proto_mapping::v1beta1::SemanticSource;

/// Optional retrieval adapter. It proposes identities; the mapping owns
/// admission and evidence. Implementations must keep a selection stable across
/// continuation pages and must never return generated source bodies.
pub(crate) trait SemanticCandidateProvider: Send + Sync {
    fn rank<'a>(
        &'a self,
        question: &'a str,
        sources: &'a [SemanticSource],
        continuation: bool,
    ) -> Pin<Box<dyn Future<Output = Result<SemanticRetrievalOutcome, String>> + Send + 'a>>;
}
