use std::collections::BTreeSet;

use super::{MemoryCoordinateData, MemoryData};

/// Counts entries once even when several memberships carry the same clock.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WriteClockCoverage {
    pub entries: usize,
    pub distinct_values: usize,
    pub single_value: Option<String>,
}

impl WriteClockCoverage {
    pub(super) fn for_memory(
        memory: &MemoryData,
        clock: impl Fn(&MemoryCoordinateData) -> Option<&str>,
    ) -> Self {
        let mut entries = 0;
        let mut values = BTreeSet::new();
        for entry in &memory.entries {
            let mut present = false;
            for coordinate in &entry.coordinates {
                if let Some(value) = clock(coordinate) {
                    present = true;
                    values.insert(value.to_owned());
                }
            }
            entries += usize::from(present);
        }
        Self {
            entries,
            distinct_values: values.len(),
            single_value: (values.len() == 1).then(|| values.pop_first().expect("one value")),
        }
    }
}
