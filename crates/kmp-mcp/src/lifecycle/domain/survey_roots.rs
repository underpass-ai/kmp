use std::path::PathBuf;

/// Where to look. Taken as values rather than read from the environment so
/// the survey can be exercised against a temporary tree.
pub struct SurveyRoots {
    pub home: PathBuf,
    pub data_home: PathBuf,
    pub working_dir: PathBuf,
    pub path_entries: Vec<PathBuf>,
}
