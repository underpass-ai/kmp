/// Whether the verdict book is opened, or only its configuration read: a
/// report must not create or prune `judgements.sqlite3` under a live
/// session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum VerdictBookAccess {
    Open,
    ConfigurationOnly,
}
