/// An accepted item that cannot be written by this call, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ApplyRejection {
    pub item_id: String,
    pub reason: String,
}
