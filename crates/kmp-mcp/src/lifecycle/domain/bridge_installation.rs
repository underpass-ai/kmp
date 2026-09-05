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
