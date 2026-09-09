use std::collections::BTreeSet;

use crate::MemoryDimensionIdentity;

/// What a caller is asking the graph for.
///
/// The read model has four axes — about, dimension and scope, five clocks,
/// typed relation — and until this existed none of them selected anything.
/// The dimension axis was the plainest waste: the application resolved it into
/// fully namespaced scope ids, carried them as far as the query, and then
/// filtered the bundle after it had already been materialised in full.
///
/// The scopes travel here as a hint, never as a contract. An adapter that
/// ignores them stays correct and only stays slow, because the application
/// still filters what comes back. That is what lets this land one adapter at
/// a time and be provable as a pure performance change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeighborhoodRequest {
    root_node_id: String,
    depth: u32,
    scopes: BTreeSet<String>,
}

impl NeighborhoodRequest {
    pub fn new(root_node_id: impl Into<String>, depth: u32) -> Self {
        Self {
            root_node_id: root_node_id.into(),
            depth,
            scopes: BTreeSet::new(),
        }
    }

    pub fn with_scopes(mut self, scopes: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.scopes = scopes
            .into_iter()
            .map(Into::into)
            .map(|scope| scope.trim().to_string())
            .filter(|scope| !scope.is_empty())
            .collect();
        self
    }

    pub fn root_node_id(&self) -> &str {
        &self.root_node_id
    }

    pub fn depth(&self) -> u32 {
        self.depth
    }

    pub fn scopes(&self) -> &BTreeSet<String> {
        &self.scopes
    }

    /// Whether traversal may descend into a node.
    ///
    /// Only dimension nodes are ever refused, and only when the caller named
    /// which ones it wanted. Everything a dimension contains — entries,
    /// evidence, the claims they support — is reached through the dimension
    /// that admits it, so narrowing at the dimension narrows everything below
    /// without ever having to reason about what a node is.
    ///
    /// Hints name a key, bare value (including its colon-delimited children),
    /// or a complete ref. A complete ref selects just that label.
    pub fn admits(&self, node_id: &str) -> bool {
        if self.scopes.is_empty() {
            return true;
        }
        let Some(identity) = MemoryDimensionIdentity::parse(node_id) else {
            return true;
        };
        self.scopes.iter().any(|scope| {
            node_id == scope
                || identity.key() == scope
                || identity.dimension_id() == scope
                || identity
                    .dimension_id()
                    .strip_prefix(scope.as_str())
                    .is_some_and(|rest| rest.starts_with(':'))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_without_scopes_admits_everything() {
        let request = NeighborhoodRequest::new("about:project", 3);

        assert!(request.admits("label:v1:about%3Aproject:timeline:timeline"));
        assert!(request.admits("project:entry:one"));
        assert_eq!(request.depth(), 3);
        assert_eq!(request.root_node_id(), "about:project");
    }

    #[test]
    fn only_dimensions_are_ever_refused() {
        let request = NeighborhoodRequest::new("about:project", 3)
            .with_scopes(["label:v1:about%3Aproject:timeline:timeline"]);

        assert!(request.admits("label:v1:about%3Aproject:timeline:timeline"));
        assert!(!request.admits("label:v1:about%3Aproject:conversation:conversation"));
        // An entry is not a dimension and is reached through one anyway.
        assert!(request.admits("project:entry:one"));
    }

    #[test]
    fn asking_for_a_dimension_asks_for_its_scopes() {
        let request = NeighborhoodRequest::new("about:project", 3).with_scopes(["conversation"]);

        assert!(request.admits("label:v1:about%3Aproject:conversation:conversation%3Aalpha"));
        assert!(!request.admits("label:v1:about%3Aproject:conversationalist:conversationalist"));
    }

    #[test]
    fn an_exact_scope_does_not_admit_its_siblings() {
        let request = NeighborhoodRequest::new("about:project", 3)
            .with_scopes(["label:v1:about%3Aproject:conversation:conversation%3Aalpha"]);

        assert!(request.admits("label:v1:about%3Aproject:conversation:conversation%3Aalpha"));
        assert!(!request.admits("label:v1:about%3Aproject:conversation:conversation%3Abeta"));
    }

    #[test]
    fn blank_scopes_are_not_a_narrowing() {
        let request = NeighborhoodRequest::new("about:project", 3).with_scopes(["", "   "]);

        assert!(request.scopes().is_empty());
        assert!(request.admits("label:v1:about%3Aproject:conversation:conversation"));
    }
}
