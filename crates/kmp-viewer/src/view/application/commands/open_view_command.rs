//! The command that opens or rehydrates a view.

/// Everything the open use case needs from a boundary: which view, onto
/// which memory, against which seen revision, and who is asking. An
/// expectation of `0` means "I expect no view to exist yet".
#[derive(Clone, Debug, Default)]
pub struct OpenViewCommand {
    /// The view to open; the default loom when absent.
    pub view_id: Option<String>,
    /// The memory to open it over, when the caller names one.
    pub about: Option<String>,
    /// The revision the caller saw, when they assert one.
    pub expected_revision: Option<u64>,
    /// Who is opening.
    pub actor: String,
    /// Why, in the caller's words.
    pub explanation: Option<String>,
}
