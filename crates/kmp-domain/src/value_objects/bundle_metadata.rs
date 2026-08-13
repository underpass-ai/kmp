#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleMetadata {
    pub revision: u64,
    pub content_hash: String,
    pub generator_version: String,
}

impl BundleMetadata {
    pub fn initial(generator_version: impl Into<String>) -> Self {
        Self {
            revision: 1,
            content_hash: "pending".to_string(),
            generator_version: generator_version.into(),
        }
    }
}
