use crate::serving::judgement_response::JudgementResponse;

/// What Jev cost one curate call: provider requests actually sent (a verdict
/// already in the book costs none), their input tokens, and how long the
/// call waited on judgement (DESIGN L4 4b), book lookups included.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JevUsage {
    pub model: String,
    pub requests: usize,
    pub input_tokens: u64,
    pub elapsed_us: u64,
}

impl JevUsage {
    pub(crate) fn new(model: &str) -> Self {
        Self {
            model: model.to_string(),
            requests: 0,
            input_tokens: 0,
            elapsed_us: 0,
        }
    }

    /// Adds one judgement's cost.
    pub(crate) fn add(&mut self, response: &JudgementResponse) {
        self.requests += response.requests;
        self.input_tokens += response.input_tokens;
        self.elapsed_us += response.elapsed_us;
    }

    /// Whole milliseconds waited, as the curate answer reports them.
    pub(crate) fn elapsed_ms(&self) -> u64 {
        self.elapsed_us / 1_000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn judgements_add_up() {
        let mut usage = JevUsage::new("jev-test");
        let mut response = JudgementResponse::empty("jev-test");
        response.requests = 2;
        response.input_tokens = 300;
        response.elapsed_us = 1_500;
        usage.add(&response);
        usage.add(&JudgementResponse::empty("jev-test"));
        assert_eq!(
            (usage.requests, usage.input_tokens, usage.elapsed_ms()),
            (2, 300, 1)
        );
    }
}
