use super::{MemoryData, WriteClockCoverage};

/// Clocks in one canonical command, not the latest state of its memory refs.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WriteClocks {
    pub entries: usize,
    pub occurred: WriteClockCoverage,
    pub observed: WriteClockCoverage,
    pub ingested: WriteClockCoverage,
    pub valid_from: WriteClockCoverage,
    pub valid_until: WriteClockCoverage,
}

impl WriteClocks {
    pub(super) fn for_memory(memory: &MemoryData) -> Self {
        Self {
            entries: memory.entries.len(),
            occurred: WriteClockCoverage::for_memory(memory, |c| c.occurred_at.as_deref()),
            observed: WriteClockCoverage::for_memory(memory, |c| c.observed_at.as_deref()),
            ingested: WriteClockCoverage::for_memory(memory, |c| c.ingested_at.as_deref()),
            valid_from: WriteClockCoverage::for_memory(memory, |c| c.valid_from.as_deref()),
            valid_until: WriteClockCoverage::for_memory(memory, |c| c.valid_until.as_deref()),
        }
    }
}
