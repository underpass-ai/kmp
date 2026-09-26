use crate::serving::verdict::Verdict;
use crate::serving::verdict_key::VerdictKey;

/// Judged verdicts kept by key (DESIGN L4 4a). The first verdict recorded
/// for a key wins for every process that shares the book: `record` never
/// replaces one, and hands back what the book holds after writing. Calls
/// block briefly on local storage; callers run them off the async runtime.
pub(crate) trait VerdictBook: Send + Sync {
    /// The verdict held for each key, in order; `None` where there is none.
    fn read(&self, keys: &[VerdictKey]) -> Result<Vec<Option<Verdict>>, String>;

    /// Keeps each verdict unless its key already holds one, and returns the
    /// verdict the book holds for each key afterwards, in order.
    fn record(&self, verdicts: &[(VerdictKey, Verdict)]) -> Result<Vec<Verdict>, String>;

    /// Drops every verdict whose key starts with `prefix` (a template or one
    /// of its versions); returns how many.
    fn invalidate(&self, prefix: &[u8]) -> Result<usize, String>;
}
