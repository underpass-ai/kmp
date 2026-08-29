use serde::Serialize;

/// Stable failure receipt for automation. A non-zero exit still emits the
/// component that failed instead of collapsing partial state into prose.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LifecycleFailureDto {
    pub action: String,
    pub status: String,
    pub failed_component: String,
    pub detail: String,
}
