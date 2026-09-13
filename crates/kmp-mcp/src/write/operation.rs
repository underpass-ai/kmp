/// The three mutually exclusive operations accepted by the native writer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WriteOperation {
    Memories,
    SearchSummaries,
    Relations,
}
