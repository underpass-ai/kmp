use std::path::{Path, PathBuf};

use redb::TableDefinition;

pub(super) const OBSERVATIONS: TableDefinition<(u64, u64), &[u8]> =
    TableDefinition::new("quality_observations");

pub fn quality_telemetry_path(data_dir: &Path) -> PathBuf {
    data_dir.join("telemetry").join("quality.redb")
}
