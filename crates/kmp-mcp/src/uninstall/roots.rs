//! Where to look for an installation.
//!
//! One concept: the four directories a survey walks. They are arguments rather
//! than environment reads so the survey can be exercised against a temporary
//! tree — which is the only way a destructive verb gets tested at all.

use std::path::PathBuf;

/// Where to look. Taken as arguments rather than read from the environment so
/// the survey can be exercised against a temporary tree.
pub struct Roots {
    pub home: PathBuf,
    pub data_home: PathBuf,
    pub working_dir: PathBuf,
    pub path_entries: Vec<PathBuf>,
}
