//! What looking for installed guide assets found.
use std::path::PathBuf;

/// The first root whose assets match this engine, or every place that was
/// looked at and why it did not serve — the lines a repair message carries
/// so a person can see what was tried before typing a path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuideAssetSearch {
    pub root: Option<PathBuf>,
    pub searched: Vec<String>,
}
