use serde::Serialize;

/// User-level lifecycle intent; setup never refreshes a marketplace.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LifecycleAction {
    Setup,
    Update,
}
