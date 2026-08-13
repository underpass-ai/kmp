use std::collections::BTreeMap;

use kmp_domain::{BundleMetadata, BundleNode, CaseId, KmpBundle, Role};

use crate::ApplicationError;

pub struct BundleAssembler;

impl BundleAssembler {
    pub fn placeholder(
        root_node_id: &str,
        role: &str,
        generator_version: &str,
    ) -> Result<KmpBundle, ApplicationError> {
        let root_node_id = CaseId::new(root_node_id)?;
        let role = Role::new(role)?;
        let summary = format!(
            "bundle for node {} role {}",
            root_node_id.as_str(),
            role.as_str()
        );

        KmpBundle::new(
            root_node_id.clone(),
            role,
            BundleNode::new(
                root_node_id.as_str(),
                "placeholder",
                format!("Node {}", root_node_id.as_str()),
                summary,
                "UNKNOWN",
                vec!["placeholder".to_string()],
                BTreeMap::new(),
            ),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            BundleMetadata::initial(generator_version),
        )
        .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::BundleAssembler;

    #[test]
    fn placeholder_builds_graph_native_bundle() {
        let bundle = BundleAssembler::placeholder("case-123", "developer", "0.1.0")
            .expect("placeholder bundle should build");

        assert_eq!(bundle.root_node_id().as_str(), "case-123");
        assert_eq!(bundle.root_node().node_kind(), "placeholder");
        assert_eq!(bundle.stats().selected_nodes(), 1);
    }
}
