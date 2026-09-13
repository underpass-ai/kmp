//! Why a memory selection was refused, in the words an operator can act on.
//!
//! Every variant is a refusal, never a correction: KMP does not migrate,
//! move, convert or overwrite a store it cannot open, and it does not quietly
//! pick a different one. The store the operator already has keeps every byte
//! it had, and the selection simply does not happen.

use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionRefusal {
    /// A selection with no path in it.
    Empty,
    /// A shell path nothing expands: MCP host configuration is not a shell.
    /// `subject` names whoever said it, because the same refusal answers the
    /// explicit environment override and the saved selection alike.
    Unexpanded {
        subject: String,
        value: String,
        home: Option<PathBuf>,
    },
    /// A relative path, which would mean a different store per working
    /// directory — the opposite of what saving a selection is for.
    Relative(String),
    /// A path the settings file cannot carry back unchanged.
    NotQuotable(String),
    /// A path that exists and is not a directory.
    NotADirectory(PathBuf),
    /// A directory that holds a store this engine will not open.
    Incompatible { path: PathBuf, reason: String },
}

impl SelectionRefusal {
    /// A `~` path, attributed to whoever wrote it.
    pub fn unexpanded(subject: impl Into<String>, value: impl Into<String>) -> Self {
        Self::Unexpanded {
            subject: subject.into(),
            value: value.into(),
            home: user_home(),
        }
    }

    /// Whether the refusal is about an existing store's bytes, which is the
    /// case that has to promise it touched nothing.
    pub fn concerns_an_existing_store(&self) -> bool {
        matches!(self, Self::Incompatible { .. } | Self::NotADirectory(_))
    }
}

/// The operator's home directory, only ever used to suggest the path they
/// meant when a shell-style one cannot be expanded.
fn user_home() -> Option<PathBuf> {
    ["HOME", "USERPROFILE"]
        .into_iter()
        .find_map(std::env::var_os)
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

impl fmt::Display for SelectionRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(
                formatter,
                "a user memory selection needs an absolute directory path"
            ),
            Self::Unexpanded {
                subject,
                value,
                home,
            } => {
                write!(
                    formatter,
                    "{subject} `{value}` starts with `~`, but MCP host configuration does not \
                     expand shell paths"
                )?;
                match home
                    .as_ref()
                    .and_then(|home| strip_home_prefix(home, value))
                {
                    Some(expanded) => write!(formatter, "; use `{}`", expanded.display()),
                    None => write!(formatter, "; use an absolute path instead"),
                }
            }
            Self::Relative(value) => write!(
                formatter,
                "memory selection `{value}` is relative; a saved selection must be an absolute \
                 path or it would name a different store from every working directory"
            ),
            Self::NotQuotable(value) => write!(
                formatter,
                "memory selection `{value}` contains a double quote or a line break, which the \
                 settings file cannot carry back unchanged"
            ),
            Self::NotADirectory(path) => write!(
                formatter,
                "`{}` exists and is not a directory; the file was left untouched",
                path.display()
            ),
            Self::Incompatible { path, reason } => write!(
                formatter,
                "`{}` holds memory this engine cannot open: {reason}; every file in it was left \
                 exactly as it was, and nothing was migrated, moved or converted",
                path.display()
            ),
        }
    }
}

fn strip_home_prefix(home: &Path, value: &str) -> Option<PathBuf> {
    Path::new(value)
        .strip_prefix("~")
        .ok()
        .map(|suffix| home.join(suffix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_incompatible_store_promises_it_was_left_alone() {
        let refusal = SelectionRefusal::Incompatible {
            path: PathBuf::from("/old/store"),
            reason: "format version 2 is not supported".to_string(),
        };
        let message = refusal.to_string();
        assert!(message.contains("/old/store"), "{message}");
        assert!(message.contains("format version 2"), "{message}");
        assert!(message.contains("left exactly as it was"), "{message}");
        assert!(message.contains("nothing was migrated"), "{message}");
        assert!(refusal.concerns_an_existing_store());
    }

    #[test]
    fn a_relative_or_shell_path_says_why_it_cannot_be_saved() {
        let relative = SelectionRefusal::Relative("memory".to_string()).to_string();
        assert!(relative.contains("absolute path"), "{relative}");

        let expandable = SelectionRefusal::Unexpanded {
            subject: "KMP_MCP_DATA_DIR value".to_string(),
            value: "~/memory".to_string(),
            home: Some(PathBuf::from("/home/u")),
        }
        .to_string();
        assert!(
            expandable.starts_with("KMP_MCP_DATA_DIR value `~/memory` starts with `~`"),
            "{expandable}"
        );
        assert!(
            expandable.contains("MCP host configuration does not expand shell paths"),
            "{expandable}"
        );
        assert!(expandable.contains("use `/home/u/memory`"), "{expandable}");

        let homeless = SelectionRefusal::Unexpanded {
            subject: "memory selection".to_string(),
            value: "~/memory".to_string(),
            home: None,
        }
        .to_string();
        assert!(homeless.contains("absolute path instead"), "{homeless}");
        assert!(!SelectionRefusal::Empty.concerns_an_existing_store());
    }
}
