use crate::curate::application::jev_usage::JevUsage;
use crate::curate::domain::found_path::FoundPath;

/// What a path search found, how much of the selection it considered, and
/// what it cost.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PathSearch {
    pub paths: Vec<FoundPath>,
    pub considered: usize,
    pub kept: usize,
    pub jev: Option<JevUsage>,
    pub warnings: Vec<String>,
}
