use serde::Serialize;

use super::memory_store_dto::MemoryStoreDto;

/// The machine's memory inventory as `kmp-mcp memories --json` prints it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MemoryInventoryDto {
    pub memories: Vec<MemoryStoreDto>,
}
