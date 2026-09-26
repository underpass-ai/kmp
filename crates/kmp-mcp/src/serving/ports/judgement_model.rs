use std::{future::Future, pin::Pin};

use crate::serving::judgement_origin::JudgementOrigin;
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

    /// `evaluate`, together with where the answers came from, for
    /// telemetry. A model that is not a recording answers remotely; a
    /// cassette overrides this.
    #[allow(clippy::type_complexity)]
    fn evaluate_traced<'a>(
        &'a self,
        request: &'a JudgementRequest,
    ) -> Pin<
        Box<dyn Future<Output = (Result<JudgementResponse, String>, JudgementOrigin)> + Send + 'a>,
    > {
        Box::pin(async move { (self.evaluate(request).await, JudgementOrigin::Remote) })
    }
}

/// A shared model is the model: decorators generic over `M: JudgementModel`
/// wrap an `Arc<dyn JudgementModel>` as they would a concrete one.
impl<T: JudgementModel + ?Sized> JudgementModel for std::sync::Arc<T> {
    fn model(&self) -> &str {
        (**self).model()
    }

    fn evaluate<'a>(
        &'a self,
        request: &'a JudgementRequest,
    ) -> Pin<Box<dyn Future<Output = Result<JudgementResponse, String>> + Send + 'a>> {
        (**self).evaluate(request)
    }

    fn evaluate_traced<'a>(
        &'a self,
        request: &'a JudgementRequest,
    ) -> Pin<
        Box<dyn Future<Output = (Result<JudgementResponse, String>, JudgementOrigin)> + Send + 'a>,
    > {
        (**self).evaluate_traced(request)
    }
}
