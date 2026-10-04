use serde::Serialize;

use super::about_event_count_dto::AboutEventCountDto;

/// One store in `memories --json`: where it is, how it is reached, what it
/// is stored as and what is inside.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MemoryStoreDto {
    pub path: String,
    pub reach: String,
    /// Whether this process, standing here, would open it.
    pub opened_here: bool,
    /// `sqlite`, an unsupported artifact label, or `null` for a store that
    /// was stamped but never written.
    pub storage: Option<String>,
    pub size_bytes: u64,
    pub last_opened: Option<String>,
    /// `false` when this engine cannot read it; `unreadable` says why.
    pub readable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unreadable: Option<String>,
    pub events: u64,
    pub last_write: Option<String>,
    pub abouts: Vec<AboutEventCountDto>,
}
