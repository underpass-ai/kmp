use std::path::{Path, PathBuf};

/// Machine-local coordination files live outside the stores they protect.
/// Keeping the lock outside the selected directory lets uninstall hold it
/// until the directory is fully gone, including on Windows. This is layout
/// knowledge, not filesystem access: the lease adapters and the survey both
/// read it from here so they cannot disagree.
pub fn store_leases_dir(data_home: &Path) -> PathBuf {
    data_home.join("kmp").join("store-leases")
}
