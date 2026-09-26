use std::collections::VecDeque;
use std::sync::Mutex;

use kmp_domain::GraphReadRevision;

use crate::serving::frozen_recall::FrozenRecall;
use crate::serving::frozen_recall_key::FrozenRecallKey;
use crate::serving::ports::frozen_recall_store::FrozenRecallStore;

/// What one server process keeps of paged recalls: 64 MiB, least recently
/// used out first. Continuations follow their first page within seconds, so
/// the budget holds hundreds of wakes of a large about while bounding the
/// process however many readers page at once.
pub(crate) const FROZEN_RECALL_BYTES: usize = 64 * 1024 * 1024;

/// A fixed charge per entry for what the estimates do not see.
const ENTRY_OVERHEAD_BYTES: usize = 256;

/// The process-local frozen recalls, in recency order (oldest first).
pub(crate) struct ProcessFrozenRecalls {
    capacity: usize,
    entries: Mutex<Kept>,
}

#[derive(Default)]
struct Kept {
    bytes: usize,
    order: VecDeque<(FrozenRecallKey, GraphReadRevision, FrozenRecall, usize)>,
}

impl Default for ProcessFrozenRecalls {
    fn default() -> Self {
        Self::with_capacity(FROZEN_RECALL_BYTES)
    }
}

impl ProcessFrozenRecalls {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            entries: Mutex::new(Kept::default()),
        }
    }

    fn kept(&self) -> std::sync::MutexGuard<'_, Kept> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[cfg(test)]
    fn kept_bytes(&self) -> usize {
        self.kept().bytes
    }
}

impl Kept {
    fn remove(&mut self, key: &FrozenRecallKey) -> Option<(GraphReadRevision, FrozenRecall)> {
        let index = self.order.iter().position(|entry| &entry.0 == key)?;
        let (_, revision, recall, bytes) = self.order.remove(index)?;
        self.bytes -= bytes;
        Some((revision, recall))
    }
}

impl FrozenRecallStore for ProcessFrozenRecalls {
    fn freeze(&self, key: FrozenRecallKey, revision: GraphReadRevision, recall: FrozenRecall) {
        let bytes = key.approximate_bytes()
            + revision.as_str().len()
            + recall.approximate_bytes()
            + ENTRY_OVERHEAD_BYTES;
        let mut kept = self.kept();
        kept.remove(&key);
        if bytes > self.capacity {
            return;
        }
        while kept.bytes + bytes > self.capacity {
            let Some((_, _, _, evicted)) = kept.order.pop_front() else {
                break;
            };
            kept.bytes -= evicted;
        }
        kept.bytes += bytes;
        kept.order.push_back((key, revision, recall, bytes));
    }

    fn thaw(&self, key: &FrozenRecallKey, revision: &GraphReadRevision) -> Option<FrozenRecall> {
        let mut kept = self.kept();
        let index = kept.order.iter().position(|entry| &entry.0 == key)?;
        if &kept.order[index].1 != revision {
            // The store moved on: this read can never be served again.
            kept.remove(key);
            return None;
        }
        let entry = kept.order.remove(index)?;
        let recall = entry.2.clone();
        kept.order.push_back(entry);
        Some(recall)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kmp_proto::v1beta1::{WakeRequest, WakeResponse};
    use kmp_proto_mapping::v1beta1::wake_query_from_proto;

    fn key(about: &str) -> FrozenRecallKey {
        FrozenRecallKey::Wake(
            wake_query_from_proto(WakeRequest {
                about: about.into(),
                ..WakeRequest::default()
            })
            .expect("wake query"),
        )
    }

    fn revision(value: &str) -> GraphReadRevision {
        GraphReadRevision::new(value).expect("revision")
    }

    fn recall(summary_bytes: usize) -> FrozenRecall {
        FrozenRecall::Wake {
            response: Box::new(WakeResponse {
                summary: "s".repeat(summary_bytes),
                ..WakeResponse::default()
            }),
            rendered: serde_json::Value::Null,
        }
    }

    #[test]
    fn a_read_is_served_only_at_the_revision_it_was_read_at() {
        let store = ProcessFrozenRecalls::default();
        store.freeze(key("a"), revision("r1"), recall(10));
        assert_eq!(store.thaw(&key("a"), &revision("r1")), Some(recall(10)));
        assert_eq!(store.thaw(&key("a"), &revision("r1")), Some(recall(10)));
        assert_eq!(store.thaw(&key("b"), &revision("r1")), None);
        // A commit in between retires the read for good.
        assert_eq!(store.thaw(&key("a"), &revision("r2")), None);
        assert_eq!(store.thaw(&key("a"), &revision("r1")), None);
        assert_eq!(store.kept_bytes(), 0);
    }

    #[test]
    fn freezing_the_same_query_again_replaces_the_read() {
        let store = ProcessFrozenRecalls::default();
        store.freeze(key("a"), revision("r1"), recall(10));
        store.freeze(key("a"), revision("r2"), recall(20));
        assert_eq!(store.thaw(&key("a"), &revision("r1")), None);
        store.freeze(key("a"), revision("r2"), recall(20));
        assert_eq!(store.thaw(&key("a"), &revision("r2")), Some(recall(20)));
        assert_eq!(store.kept().order.len(), 1);
    }

    #[test]
    fn the_least_recently_used_read_leaves_first_within_the_byte_budget() {
        let one = key("a").approximate_bytes() + 2 + 1_000 + ENTRY_OVERHEAD_BYTES + 16;
        let store = ProcessFrozenRecalls::with_capacity(one * 2);
        store.freeze(key("a"), revision("r1"), recall(1_000));
        store.freeze(key("b"), revision("r1"), recall(1_000));
        // Reading `a` makes `b` the oldest.
        assert!(store.thaw(&key("a"), &revision("r1")).is_some());
        store.freeze(key("c"), revision("r1"), recall(1_000));
        assert!(store.thaw(&key("b"), &revision("r1")).is_none());
        assert!(store.thaw(&key("a"), &revision("r1")).is_some());
        assert!(store.thaw(&key("c"), &revision("r1")).is_some());
        assert!(store.kept_bytes() <= one * 2);
    }

    #[test]
    fn a_read_larger_than_the_budget_is_never_kept() {
        let store = ProcessFrozenRecalls::with_capacity(4_096);
        store.freeze(key("a"), revision("r1"), recall(100));
        store.freeze(key("b"), revision("r1"), recall(10_000));
        assert!(store.thaw(&key("b"), &revision("r1")).is_none());
        assert!(store.thaw(&key("a"), &revision("r1")).is_some());
        // Replacing a kept read by one too large drops the old one too.
        store.freeze(key("a"), revision("r1"), recall(10_000));
        assert!(store.thaw(&key("a"), &revision("r1")).is_none());
        assert_eq!(store.kept_bytes(), 0);
        let ask = FrozenRecall::Ask {
            response: Box::default(),
            rendered: serde_json::Value::Null,
        };
        store.freeze(key("c"), revision("r1"), ask.clone());
        assert_eq!(store.thaw(&key("c"), &revision("r1")), Some(ask));
    }
}
