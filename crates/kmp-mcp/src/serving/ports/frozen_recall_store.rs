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
///
/// A first page whose core had to be shortened proposes a restart: the same
/// call without a cursor and with a larger byte ceiling. The byte ceiling is
/// not part of the key, so that restart reads exactly what the first page
/// read. Such a read is kept `awaiting_restart`, and the one call without a
/// cursor it may serve is taken with `thaw_restart`, once.
pub(crate) trait FrozenRecallStore: Send + Sync {
    fn freeze(
        &self,
        key: FrozenRecallKey,
        revision: GraphReadRevision,
        recall: FrozenRecall,
        awaiting_restart: bool,
    );

    fn thaw(&self, key: &FrozenRecallKey, revision: &GraphReadRevision) -> Option<FrozenRecall>;

    /// The read, when it awaits its restart at this revision; the restart
    /// is served once, and a later call without a cursor reads again.
    fn thaw_restart(
        &self,
        key: &FrozenRecallKey,
        revision: &GraphReadRevision,
    ) -> Option<FrozenRecall>;
}
