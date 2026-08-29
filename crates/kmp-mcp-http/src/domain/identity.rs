use std::collections::BTreeSet;

/// Authenticated actor and its bounded KMP grants.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Identity {
    pub subject: String,
    pub workspace: Option<String>,
    pub scopes: BTreeSet<String>,
    pub abouts: BTreeSet<String>,
    pub scope_ids: BTreeSet<String>,
    pub ref_prefixes: BTreeSet<String>,
}

impl Identity {
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.contains(scope)
    }
}
