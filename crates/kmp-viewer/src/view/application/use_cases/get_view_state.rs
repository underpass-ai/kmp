//! Reading the view without changing it.

use crate::view::domain::{ViewId, ViewState};
use crate::view::ports::ViewSessionStore;

/// Reads the semantic state of a view — never its pixels. Reading counts as
/// touching, so a view someone is watching does not expire under them.
pub struct GetViewState<'a, Store> {
    /// Where sessions live.
    pub store: &'a Store,
}

impl<Store: ViewSessionStore> GetViewState<'_, Store> {
    /// The state under the id, if a view is open there.
    pub fn execute(&self, view_id: Option<&str>) -> Option<ViewState> {
        self.store.read(&ViewId::or_default(view_id))
    }
}
