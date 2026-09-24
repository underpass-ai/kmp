#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JevUsage {
    pub model: String,
    pub requests: usize,
    pub input_tokens: u64,
}
