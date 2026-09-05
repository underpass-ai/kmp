use std::path::PathBuf;

use super::lifecycle_error::LifecycleError;

/// What one lifecycle run did about the lexical-bridge table.
///
/// Every variant is a success: the table is an aid to retrieval, so a
/// release that publishes none and a network that will not answer both leave
/// a converged installation behind. What they must not do is leave it
/// unsaid, which is the whole point of reporting this in the receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BridgeInstallation {
    /// The table was written; `ask` now crosses languages on this machine.
    Installed {
        path: PathBuf,
        bytes: usize,
        sha256: String,
        source: String,
    },
    /// The installed table already had the published digest.
    AlreadyCurrent { path: PathBuf, sha256: String },
    /// The operator asked for no table.
    Declined,
    /// No table was installed and the convergence proceeded regardless.
    Unavailable { reason: String },
}

impl BridgeInstallation {
    /// Turn anything that went wrong into a reported outcome. Nothing about
    /// a retrieval aid justifies failing a convergence that is already
    /// proved, so this is the only way an error leaves this bounded piece.
    pub fn unavailable(error: LifecycleError) -> Self {
        Self::Unavailable {
            reason: error.to_string(),
        }
    }

    /// One line for a human reading a receipt.
    pub fn summary(&self) -> String {
        match self {
            Self::Installed {
                path,
                bytes,
                source,
                ..
            } => format!(
                "installed {source} ({bytes} bytes) at {}; ask now crosses languages",
                path.display()
            ),
            Self::AlreadyCurrent { path, .. } => {
                format!("already current at {}", path.display())
            }
            Self::Declined => {
                "declined; ask matches within one language on this machine".to_string()
            }
            Self::Unavailable { reason } => {
                format!("not installed: {reason}; ask matches within one language")
            }
        }
    }

    /// Whether `ask` can cross languages after this run. A run that declined
    /// says nothing about a table installed earlier, so it is not an answer
    /// to this question and reports `false` only about itself.
    pub fn table_is_present(&self) -> bool {
        matches!(self, Self::Installed { .. } | Self::AlreadyCurrent { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed() -> BridgeInstallation {
        BridgeInstallation::Installed {
            path: PathBuf::from("/home/data/kmp/lexical-bridge.kmpb"),
            bytes: 6_815_744,
            sha256: "abc123".to_string(),
            source: "kmp-lexical-bridge.kmpb from v0.13.0".to_string(),
        }
    }

    #[test]
    fn an_installed_table_names_what_arrived_and_where() {
        let summary = installed().summary();

        assert!(
            summary.contains("kmp-lexical-bridge.kmpb from v0.13.0"),
            "{summary}"
        );
        assert!(summary.contains("6815744 bytes"), "{summary}");
        assert!(
            summary.contains("/home/data/kmp/lexical-bridge.kmpb"),
            "{summary}"
        );
    }

    #[test]
    fn only_a_table_that_is_there_crosses_languages() {
        let current = BridgeInstallation::AlreadyCurrent {
            path: PathBuf::from("/home/data/kmp/lexical-bridge.kmpb"),
            sha256: "abc123".to_string(),
        };

        assert!(installed().table_is_present());
        assert!(current.table_is_present());
        assert!(!BridgeInstallation::Declined.table_is_present());
        assert!(
            !BridgeInstallation::unavailable(LifecycleError::Network("no answer".to_string()))
                .table_is_present()
        );
    }

    #[test]
    fn a_table_already_current_says_so_without_repeating_its_digest() {
        let summary = BridgeInstallation::AlreadyCurrent {
            path: PathBuf::from("/home/data/kmp/lexical-bridge.kmpb"),
            sha256: "abc123".to_string(),
        }
        .summary();

        assert_eq!(
            summary,
            "already current at /home/data/kmp/lexical-bridge.kmpb"
        );
    }

    #[test]
    fn declining_says_what_ask_will_do_instead() {
        assert!(
            BridgeInstallation::Declined
                .summary()
                .contains("matches within one language")
        );
    }

    /// The rule this type exists for: what went wrong is carried as words,
    /// never as a failure.
    #[test]
    fn a_failure_becomes_a_reason_a_reader_can_act_on() {
        let outcome = BridgeInstallation::unavailable(LifecycleError::Io {
            path: PathBuf::from("/home/data/kmp/lexical-bridge.kmpb"),
            detail: "read-only file system".to_string(),
        });

        let summary = outcome.summary();
        assert!(summary.contains("read-only file system"), "{summary}");
        assert!(summary.contains("matches within one language"), "{summary}");
    }
}
