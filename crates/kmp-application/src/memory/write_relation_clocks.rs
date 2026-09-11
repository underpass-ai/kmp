use super::MemoryData;

/// Coverage of semantic relations in the accepted command, not their endpoints.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WriteRelationClocks {
    pub relations: usize,
    pub occurred: usize,
    pub observed: usize,
    pub ingested: usize,
    pub valid_from: usize,
    pub valid_until: usize,
}

impl WriteRelationClocks {
    pub(super) fn for_memory(memory: &MemoryData) -> Self {
        let mut coverage = Self::default();
        for relation in &memory.relations {
            if relation.semantic_class == "structural" {
                continue;
            }
            coverage.relations += 1;
            if let Some(clocks) = &relation.clocks {
                coverage.occurred += usize::from(clocks.occurred_at.is_some());
                coverage.observed += usize::from(clocks.observed_at.is_some());
                coverage.ingested += usize::from(clocks.ingested_at.is_some());
                coverage.valid_from += usize::from(clocks.valid_from.is_some());
                coverage.valid_until += usize::from(clocks.valid_until.is_some());
            }
        }
        coverage
    }
}
