use std::{future::Future, pin::Pin};

use crate::serving::judgement_request::JudgementRequest;
use crate::serving::judgement_response::JudgementResponse;

/// Optional remote judgement. It answers typed questions about text it is
/// given; it never writes memory, and its answers are proposals the caller
/// validates and a writer justifies.
pub(crate) trait JudgementModel: Send + Sync {
    /// The pinned model every answer must come from.
    fn model(&self) -> &str;

    fn evaluate<'a>(
        &'a self,
        request: &'a JudgementRequest,
    ) -> Pin<Box<dyn Future<Output = Result<JudgementResponse, String>> + Send + 'a>>;
}
