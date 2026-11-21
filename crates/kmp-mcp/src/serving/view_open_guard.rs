//! What `kmp_view_open` says about an `expected_revision` it did not check.

use kmp_viewer::{ViewRegistry, view_state_dto};

/// The guard binds only an open that changes the about. A stale revision on
/// an open of the about the view already shows is not refused — refusing it
/// would break callers that reopen with an old revision — but it is said,
/// so the call does not read as checked (E26).
pub(crate) fn unchecked_guard(
    arguments: &serde_json::Value,
    about: &str,
    expected: Option<u64>,
) -> Option<String> {
    let expected = expected?;
    let view_id = super::view_tools::view_id_of(arguments);
    let before = view_state_dto(&ViewRegistry::shared().view_state(Some(&view_id))?);
    (before.about.as_deref() == Some(about) && before.view_revision != expected).then(|| {
        format!(
            "expected_revision {expected} was not checked: the view is at revision {} and \
             already shows `{about}`, so this open changed nothing and refused nothing. The guard \
             applies only to an open that changes the about.",
            before.view_revision
        )
    })
}
