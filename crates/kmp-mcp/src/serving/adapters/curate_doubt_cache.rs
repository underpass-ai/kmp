use std::collections::VecDeque;
use std::sync::Mutex;

use crate::curate::domain::frozen_check::FrozenCheck;

const KEPT: usize = 16;

/// Jev's pre-write check, doubts and checking model, by the digest of the accepted items, so resuming
/// an apply after a neighbourhood review reads the same doubts instead of
/// asking again. Oldest out first.
#[derive(Default)]
pub(crate) struct CurateDoubtCache {
    doubts: Mutex<VecDeque<(String, FrozenCheck)>>,
}

impl CurateDoubtCache {
    pub(crate) fn insert(&self, digest: String, check: FrozenCheck) {
        let mut kept = self
            .doubts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        kept.retain(|(key, _)| *key != digest);
        if kept.len() >= KEPT {
            kept.pop_front();
        }
        kept.push_back((digest, check));
    }

    pub(crate) fn get(&self, digest: &str) -> Option<FrozenCheck> {
        let kept = self
            .doubts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        kept.iter()
            .find(|(key, _)| key == digest)
            .map(|(_, check)| check.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frozen_set_is_read_back_and_the_oldest_leaves_first() {
        let cache = CurateDoubtCache::default();
        for n in 0..17 {
            cache.insert(format!("d{n}"), FrozenCheck::default());
        }
        assert!(cache.get("d0").is_none());
        assert_eq!(cache.get("d16"), Some(FrozenCheck::default()));
    }
}
