use std::collections::BTreeMap;

/// Coordinate frontiers this process already knows, each at the about
/// revision it holds for (DESIGN L6, write in O(delta)).
///
/// A frontier is the highest sequence a `(dimension, scope)` coordinate of an
/// about holds. Reading it walks every entry the scope contains, so a
/// session that writes one entry at a time would pay the size of the scope
/// on every write. A frontier is kept only when this process's own write
/// moved the about exactly one revision past the one it read it at, so no
/// other writer can have added to it; any other revision is read again.
#[derive(Debug, Default)]
pub(super) struct SequenceFrontierCache {
    entries: BTreeMap<(String, String, String), (u64, u32)>,
}

/// Abouts rarely hold more than a few dozen scopes; past this the cache
/// starts over rather than growing with every about a process writes.
const MAX_ENTRIES: usize = 4096;

impl SequenceFrontierCache {
    /// The frontier of `key` in `about`, if it is known at `revision`.
    pub(super) fn lookup(&self, about: &str, key: &(String, String), revision: u64) -> Option<u32> {
        self.entries
            .get(&(about.to_string(), key.0.clone(), key.1.clone()))
            .filter(|(held, _)| *held == revision)
            .map(|(_, frontier)| *frontier)
    }

    pub(super) fn remember(
        &mut self,
        about: &str,
        frontiers: &BTreeMap<(String, String), u32>,
        revision: u64,
    ) {
        if self.entries.len() + frontiers.len() > MAX_ENTRIES {
            self.entries.clear();
        }
        for (key, frontier) in frontiers {
            self.entries.insert(
                (about.to_string(), key.0.clone(), key.1.clone()),
                (revision, *frontier),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frontier_holds_only_at_the_revision_it_was_kept_at() {
        let mut cache = SequenceFrontierCache::default();
        let key = ("work".to_string(), "work:main".to_string());
        cache.remember("a", &BTreeMap::from([(key.clone(), 7)]), 3);
        assert_eq!(cache.lookup("a", &key, 3), Some(7));
        assert_eq!(cache.lookup("a", &key, 4), None);
        assert_eq!(cache.lookup("b", &key, 3), None);
    }
}
