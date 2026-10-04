use crate::lifecycle::application::dto::about_event_count_dto::AboutEventCountDto;
use crate::lifecycle::application::dto::memory_inventory_dto::MemoryInventoryDto;
use crate::lifecycle::application::dto::memory_store_dto::MemoryStoreDto;
use crate::lifecycle::domain::memory_inventory_entry::MemoryInventoryEntry;
use crate::lifecycle::domain::store_contents::StoreContents;

/// Maps the inventory onto the stable `memories --json` DTO.
#[derive(Clone, Copy, Debug, Default)]
pub struct MemoryInventoryMapper;

impl MemoryInventoryMapper {
    pub fn to_dto(entries: &[MemoryInventoryEntry]) -> MemoryInventoryDto {
        MemoryInventoryDto {
            memories: entries.iter().map(Self::store).collect(),
        }
    }

    fn store(entry: &MemoryInventoryEntry) -> MemoryStoreDto {
        let record = &entry.record;
        let (readable, unreadable, last_write, abouts) = match &entry.contents {
            StoreContents::Read { abouts, last_write } => (
                true,
                None,
                last_write.clone(),
                abouts
                    .iter()
                    .map(|about| AboutEventCountDto {
                        about: about.about().to_string(),
                        events: about.events(),
                    })
                    .collect(),
            ),
            StoreContents::Unreadable { reason } => (false, Some(reason.clone()), None, Vec::new()),
        };
        MemoryStoreDto {
            path: record.path.display().to_string(),
            reach: record.reach.as_str().to_string(),
            opened_here: entry.opened_here,
            storage: record.storage.as_ref().map(|storage| storage.label()),
            size_bytes: record.size.bytes(),
            last_opened: record.last_opened.clone(),
            readable,
            unreadable,
            events: entry.contents.total_events(),
            last_write,
            abouts,
        }
    }
}
