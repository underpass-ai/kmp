use std::collections::BTreeMap;

use crate::NodeProjection;
use crate::value_objects::Provenance;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleNode {
    node_id: String,
    node_kind: String,
    title: String,
    summary: String,
    status: String,
    labels: Vec<String>,
    properties: BTreeMap<String, String>,
    provenance: Option<Provenance>,
}

impl BundleNode {
    pub fn new(
        node_id: impl Into<String>,
        node_kind: impl Into<String>,
        title: impl Into<String>,
        summary: impl Into<String>,
        status: impl Into<String>,
        labels: Vec<String>,
        properties: BTreeMap<String, String>,
    ) -> Self {
        Self {
            node_id: node_id.into(),
            node_kind: node_kind.into(),
            title: title.into(),
            summary: summary.into(),
            status: status.into(),
            labels,
            properties,
            provenance: None,
        }
    }

    pub fn with_provenance(mut self, provenance: Provenance) -> Self {
        self.provenance = Some(provenance);
        self
    }

    pub fn from_projection(node: &NodeProjection) -> Self {
        Self {
            node_id: node.node_id.clone(),
            node_kind: node.node_kind.clone(),
            title: node.title.clone(),
            summary: node.summary.clone(),
            status: node.status.clone(),
            labels: node.labels.clone(),
            properties: node.properties.clone(),
            provenance: node.provenance.clone(),
        }
    }

    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    pub fn node_kind(&self) -> &str {
        &self.node_kind
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn summary(&self) -> &str {
        &self.summary
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    pub fn properties(&self) -> &BTreeMap<String, String> {
        &self.properties
    }

    pub fn provenance(&self) -> Option<&Provenance> {
        self.provenance.as_ref()
    }
}
