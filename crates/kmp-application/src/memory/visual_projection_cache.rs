use std::{collections::VecDeque, sync::Arc};

use super::{
    VisualProjectionResult, visual_projection_identity::VisualProjectionIdentity,
    visual_projection_retention::retained_bytes,
};

const MAX_ENTRIES: usize = 8;
const MAX_RETAINED_BYTES: usize = 32 * 1024 * 1024;

/// A service-local LRU of successful projections. No transport response,
/// capability or caller credential enters this cache. Oversized results are
/// served normally without retaining them.
#[derive(Debug, Default)]
pub(super) struct VisualProjectionCache {
    entries: VecDeque<(VisualProjectionIdentity, Arc<VisualProjectionResult>, usize)>,
    retained: usize,
}

impl VisualProjectionCache {
    pub(super) fn get(
        &mut self,
        identity: &VisualProjectionIdentity,
    ) -> Option<Arc<VisualProjectionResult>> {
        let position = self
            .entries
            .iter()
            .position(|(key, _, _)| key == identity)?;
        let entry = self.entries.remove(position)?;
        let result = Arc::clone(&entry.1);
        self.entries.push_back(entry);
        Some(result)
    }

    pub(super) fn put(
        &mut self,
        identity: VisualProjectionIdentity,
        result: &VisualProjectionResult,
    ) {
        // An older in-flight snapshot may finish after a newer one. Its key
        // remains its own revision; at worst this evicts a useful entry.
        self.entries
            .retain(|(key, _, _)| key.revision == identity.revision && key != &identity);
        self.retained = self.entries.iter().map(|entry| entry.2).sum();
        let bytes = retained_bytes(&identity, result);
        if bytes > MAX_RETAINED_BYTES {
            return;
        }
        while self.entries.len() >= MAX_ENTRIES
            || self.retained.saturating_add(bytes) > MAX_RETAINED_BYTES
        {
            if let Some((_, _, charge)) = self.entries.pop_front() {
                self.retained -= charge;
            }
        }
        self.retained += bytes;
        self.entries
            .push_back((identity, Arc::new(result.clone()), bytes));
    }
}

#[cfg(test)]
#[path = "visual_projection_cache_tests.rs"]
mod tests;
