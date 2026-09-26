use kmp_domain::GraphReadRevision;

use crate::serving::frozen_recall::FrozenRecall;
use crate::serving::frozen_recall_key::FrozenRecallKey;

/// Where the first page of a paged recall leaves its read, so that its
/// continuations cut their pages from it instead of repeating the kernel
/// read, the ranking and the remote channels.
///
/// A read is only ever handed back for the revision it was read at: a
/// continuation whose store moved on in between (any commit, by this process
/// or another) finds nothing and reads again, exactly as without the store.
/// Implementations may forget anything at any time; a miss is always safe.
pub(crate) trait FrozenRecallStore: Send + Sync {
    fn freeze(&self, key: FrozenRecallKey, revision: GraphReadRevision, recall: FrozenRecall);

    fn thaw(&self, key: &FrozenRecallKey, revision: &GraphReadRevision) -> Option<FrozenRecall>;
}
