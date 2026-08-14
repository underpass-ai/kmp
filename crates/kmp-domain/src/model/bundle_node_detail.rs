use crate::NodeDetailProjection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleNodeDetail {
    node_id: String,
    detail: String,
    content_hash: String,
    revision: u64,
}

impl BundleNodeDetail {
    pub fn new(
        node_id: impl Into<String>,
        detail: impl Into<String>,
        content_hash: impl Into<String>,
        revision: u64,
    ) -> Self {
        Self {
            node_id: node_id.into(),
            detail: detail.into(),
            content_hash: content_hash.into(),
            revision,
        }
    }

    pub fn from_projection(detail: &NodeDetailProjection) -> Self {
        Self {
            node_id: detail.node_id.clone(),
            detail: detail.detail.clone(),
            content_hash: detail.content_hash.clone(),
            revision: detail.revision,
        }
    }

    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }

    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }
}
