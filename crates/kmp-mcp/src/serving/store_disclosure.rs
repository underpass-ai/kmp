//! Which memory this session opened, said to the agent (#903).
//!
//! Selection is logged to the store's own log, which no agent reads. When a
//! machine-wide setting sent every session to one project's store, four
//! sessions woke onto the wrong memory and none of them could tell. The
//! session therefore names its store once in the `initialize` instructions,
//! and every `kmp_wake` carries it, so an empty wake reads as "this store has
//! nothing on that about" rather than "there is nothing".

use std::path::PathBuf;

use serde_json::{Value, json};

/// The store an embedded session opened and the rule that chose it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreDisclosure {
    path: PathBuf,
    rule: &'static str,
}

impl StoreDisclosure {
    pub fn of(resolved: &kmp_embedded::ResolvedDataDir) -> Self {
        Self {
            path: resolved.path().to_path_buf(),
            rule: resolved.rule_name(),
        }
    }

    /// One sentence appended to the session instructions.
    pub(crate) fn instruction(&self) -> String {
        format!(
            " Memory store: {} (chosen by {}); a wake that finds nothing speaks for this store only, \
             and `kmp-mcp info` explains the selection.",
            self.path.display(),
            self.rule
        )
    }

    /// The line a failed wake starts with, so the refusal names where it
    /// looked.
    pub(crate) fn error_prefix(&self) -> String {
        format!(
            "Memory store: {} (chosen by {}).\n",
            self.path.display(),
            self.rule
        )
    }

    /// The `store` field a wake carries.
    pub(crate) fn field(&self) -> Value {
        json!({"path": self.path.display().to_string(), "rule": self.rule})
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use kmp_embedded::ResolvedDataDir;

    use super::*;

    #[test]
    fn names_the_store_and_the_rule_that_chose_it() {
        let disclosure = StoreDisclosure::of(&ResolvedDataDir::Worktree {
            path: PathBuf::from("/repo/.kernel"),
            checkout: PathBuf::from("/repo-feature"),
        });
        assert_eq!(
            disclosure.field(),
            json!({"path": "/repo/.kernel", "rule": "worktree"})
        );
        let sentence = disclosure.instruction();
        assert!(sentence.contains("Memory store: /repo/.kernel (chosen by worktree)"));
        assert!(sentence.contains("kmp-mcp info"));
    }
}
