use serde_json::Value;

/// One validated relabel: the kernel request it compiled to, and the pairs
/// it asked for, so the result can say what the caller meant beside what
/// the kernel did.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RelabelPlan {
    pub(crate) about: String,
    pub(crate) reference: String,
    pub(crate) dry_run: bool,
    /// The labels to add, each `{ key, value }`, in argument order.
    pub(crate) add: Vec<Value>,
    /// The labels to take off, each `{ key, value }`, in argument order.
    pub(crate) remove: Vec<Value>,
    /// The `kmp_relabel` backend arguments: the kernel's own shape.
    pub(crate) relabel_arguments: Value,
}
