use std::collections::VecDeque;
use std::sync::Mutex;

use crate::curate::domain::apply_doubt::ApplyDoubt;

const KEPT: usize = 16;

/// Jev's pre-write doubts by the digest of the accepted items, so resuming
/// an apply after a neighbourhood review reads the same doubts instead of
/// asking again. Oldest out first.
#[derive(Default)]
pub(crate) struct CurateDoubtCache {
    doubts: Mutex<VecDeque<(String, Vec<ApplyDoubt>)>>,
}

impl CurateDoubtCache {
    pub(crate) fn insert(&self, digest: String, doubts: Vec<ApplyDoubt>) {
        let mut kept = self
            .doubts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        kept.retain(|(key, _)| *key != digest);
        if kept.len() >= KEPT {
            kept.pop_front();
        }
        kept.push_back((digest, doubts));
    }

    pub(crate) fn get(&self, digest: &str) -> Option<Vec<ApplyDoubt>> {
        let kept = self
            .doubts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        kept.iter()
            .find(|(key, _)| key == digest)
            .map(|(_, doubts)| doubts.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frozen_set_is_read_back_and_the_oldest_leaves_first() {
        let cache = CurateDoubtCache::default();
        for n in 0..17 {
            cache.insert(format!("d{n}"), Vec::new());
        }
        assert!(cache.get("d0").is_none());
        assert_eq!(cache.get("d16"), Some(Vec::new()));
    }
}
