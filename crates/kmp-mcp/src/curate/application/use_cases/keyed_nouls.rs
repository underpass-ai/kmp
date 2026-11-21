use std::pin::Pin;

use super::scripted_judgement::Scripted;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::judgement_response::JudgementResponse;
use crate::serving::ports::judgement_model::JudgementModel;

/// [`Scripted`], except that a noul whose key starts with one of `yes`'s
/// prefixes answers that prefix's probability: a test can let companions
/// read yes while other nouls read no.
pub(crate) struct KeyedNouls {
    pub inner: Scripted,
    pub yes: Vec<(&'static str, f64)>,
}

impl JudgementModel for KeyedNouls {
    fn model(&self) -> &str {
        self.inner.model()
    }

    fn evaluate<'a>(
        &'a self,
        request: &'a JudgementRequest,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<JudgementResponse, String>> + Send + 'a>>
    {
        Box::pin(async move {
            let mut response = self.inner.evaluate(request).await?;
            for (key, answer) in &mut response.answers {
                if let JudgementAnswer::Noul { yes } = answer
                    && let Some((_, keyed)) =
                        self.yes.iter().find(|(prefix, _)| key.starts_with(prefix))
                {
                    *yes = *keyed;
                }
            }
            Ok(response)
        })
    }
}
