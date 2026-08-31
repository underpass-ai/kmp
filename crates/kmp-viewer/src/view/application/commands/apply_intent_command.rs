//! The command that moves a view.

use crate::view::application::dto::ViewIntentDto;

/// Everything the apply use case needs from a boundary: the intent, the
/// concurrency expectation, the idempotency key, who is moving and why —
/// and, resolved by the boundary before the aggregate is reached, the
/// requested names the mounted store could not honor.
#[derive(Clone, Debug, Default)]
pub struct ApplyIntentCommand {
    /// The view to move; the default loom when absent.
    pub view_id: Option<String>,
    /// The revision the caller saw, when they assert one.
    pub expected_revision: Option<u64>,
    /// The key a retried intent travels under.
    pub idempotency_key: Option<String>,
    /// The intent's digest when the boundary took it before resolving
    /// store-local names; computed from `intent` when absent.
    pub intent_digest: Option<String>,
    /// What to change.
    pub intent: ViewIntentDto,
    /// Who is moving the view.
    pub actor: String,
    /// Why, in the mover's words.
    pub explanation: Option<String>,
    /// Requested names the store could not resolve, reported back rather
    /// than drawn as if they were data.
    pub unhonored: Vec<String>,
}
