use kmp_domain::ContextUpdatedEvent;

/// The last event of the store's log: its position and the aggregate,
/// revision and content hash it carries (the tail hash).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogTail {
    pub(crate) position: u64,
    event: Option<(String, u64, String)>,
}

impl LogTail {
    pub(crate) fn of(position: u64, event: Option<&ContextUpdatedEvent>) -> Self {
        Self {
            position,
            event: event.map(|event| {
                (
                    event.root_node_id.clone(),
                    event.revision,
                    event.content_hash.clone(),
                )
            }),
        }
    }
}
