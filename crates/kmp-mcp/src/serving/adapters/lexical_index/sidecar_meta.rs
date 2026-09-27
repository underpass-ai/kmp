/// Where the sidecar stands: which derivation wrote it (`version`), under
/// which reading (`profile`), the last event of the store's log it has
/// followed (`position`) and a witness of that event (`tail`), so a store
/// whose log was replaced under the same length is rebuilt, not trusted.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct SidecarMeta {
    pub(super) version: String,
    pub(super) profile: String,
    pub(super) position: u64,
    pub(super) tail: String,
}

impl SidecarMeta {
    /// Whether the sidecar was written by this derivation under this reading.
    pub(super) fn matches(&self, version: &str, profile: &str) -> bool {
        self.version == version && self.profile == profile
    }
}
