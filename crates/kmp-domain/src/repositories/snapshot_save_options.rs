#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SnapshotSaveOptions {
    ttl_seconds: Option<u64>,
}

impl SnapshotSaveOptions {
    pub const fn new(ttl_seconds: Option<u64>) -> Self {
        Self { ttl_seconds }
    }

    pub const fn ttl_seconds(self) -> Option<u64> {
        self.ttl_seconds
    }
}
