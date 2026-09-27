use std::collections::BTreeSet;

use kmp_adapter_embedded::{BundleHeader, EmbeddedKernelStore, HeadStream};
use kmp_domain::PortError;

use crate::log_tail::LogTail;

/// The authored head read from one snapshot of the store: the bundle a
/// full export writes, and where the log stood.
pub(crate) struct ReadHead {
    pub(crate) log: LogTail,
    pub(crate) stream: HeadStream,
}

impl ReadHead {
    /// Every event but those of `excluded` abouts, from one snapshot.
    pub(crate) async fn read(
        store: &EmbeddedKernelStore,
        excluded: &[String],
    ) -> Result<Self, PortError> {
        let excluded = excluded.iter().cloned().collect::<BTreeSet<_>>();
        store
            .read_points(move |reads| {
                let last = reads.last_event_sequence()?;
                let mut stream = HeadStream::default();
                let mut tail = None;
                for position in 1..=last {
                    let Some(event) = reads.event(position)? else {
                        continue;
                    };
                    if !excluded.contains(&event.root_node_id) {
                        stream.extend([&event])?;
                    }
                    if position == last {
                        tail = Some(event);
                    }
                }
                Ok(Self {
                    log: LogTail::of(last, tail.as_ref()),
                    stream,
                })
            })
            .await
    }

    /// The head bundle and its header.
    pub(crate) fn bundle(&self) -> Result<(String, BundleHeader), PortError> {
        self.stream.encode_with_header()
    }
}
