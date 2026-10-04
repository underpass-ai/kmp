use super::memory_record::MemoryRecord;
use super::store_contents::StoreContents;

/// One store in the machine's inventory: the survey's record, whether this
/// process would open it, and what is inside.
#[derive(Debug, Clone)]
pub struct MemoryInventoryEntry {
    pub record: MemoryRecord,
    pub opened_here: bool,
    pub contents: StoreContents,
}
