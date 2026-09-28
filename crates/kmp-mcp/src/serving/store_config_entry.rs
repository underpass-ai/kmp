use std::path::PathBuf;

use super::store_config_state::StoreConfigState;

/// One optional file a store consults, as the store config acknowledgement
/// read it: the same verdict the server logs on `kmp_mcp::store_config`,
/// with the values that took effect (#887).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StoreConfigEntry {
    pub(crate) name: String,
    pub(crate) path: Option<PathBuf>,
    pub(crate) state: StoreConfigState,
    /// The effective settings worth naming, as `key value` phrases.
    pub(crate) effective: Vec<String>,
}
