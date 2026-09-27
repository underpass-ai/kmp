//! What this process last published to a commit-native bundle, so the next
//! guarded write can prove the two histories still agree from their tails and
//! extend the bundle by the events it added (DESIGN L6, write in O(delta)).

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use kmp_adapter_embedded::{EmbeddedKernelStore, HeadStream};
use kmp_domain::PortError;

use crate::file_stamp::FileStamp;
use crate::log_tail::LogTail;
use crate::read_head::ReadHead;

/// The authored head of a store at one log position, as this process
/// published it to the committed bundle.
#[derive(Debug, Clone)]
pub(crate) struct CommittedTail {
    file: FileStamp,
    log: LogTail,
    stream: HeadStream,
}

impl CommittedTail {
    /// What a publish of `head` to `path` leaves behind; `None` when the
    /// file cannot be stamped, which only costs the next write a full check.
    pub(crate) fn published(head: ReadHead, path: &Path) -> Option<Self> {
        Some(Self {
            file: FileStamp::of(path)?,
            log: head.log,
            stream: head.stream,
        })
    }

    /// Whether the committed file and the store stand exactly where this
    /// process left them: the file unmoved, and the log ending at the same
    /// position on the same event. Then the two are still the one history
    /// the last guarded write proved equal, and no export is needed.
    pub(crate) async fn holds(
        &self,
        store: &EmbeddedKernelStore,
        path: &Path,
    ) -> Result<bool, PortError> {
        if FileStamp::of(path).as_ref() != Some(&self.file) {
            return Ok(false);
        }
        Ok(tail_at(store, self.log.position).await? == self.log)
    }

    /// The head extended by what the log holds after this tail, or `None`
    /// when the event this tail ends on is no longer there: the history
    /// moved under the guarded write.
    pub(crate) async fn extended(
        self,
        store: &EmbeddedKernelStore,
        excluded: &[String],
    ) -> Result<Option<ReadHead>, PortError> {
        let excluded = excluded.iter().cloned().collect::<BTreeSet<_>>();
        let Self {
            log: from,
            mut stream,
            ..
        } = self;
        store
            .read_points(move |reads| {
                let at = reads.event(from.position)?;
                if from.position > 0 && LogTail::of(from.position, at.as_ref()) != from {
                    return Ok(None);
                }
                let last = reads.last_event_sequence()?;
                if last < from.position {
                    return Ok(None);
                }
                let mut tail = at;
                for position in from.position + 1..=last {
                    let Some(event) = reads.event(position)? else {
                        continue;
                    };
                    if !excluded.contains(&event.root_node_id) {
                        stream.extend([&event])?;
                    }
                    tail = Some(event);
                }
                Ok(Some(ReadHead {
                    log: LogTail::of(last, tail.as_ref()),
                    stream,
                }))
            })
            .await
    }

    /// Whether the committed file is still the one this process wrote:
    /// unmoved, or rewritten with the same bytes.
    pub(crate) fn file_is_ours(&self, path: &Path) -> Result<bool, PortError> {
        if FileStamp::of(path).as_ref() == Some(&self.file) {
            return Ok(true);
        }
        match fs::read_to_string(path) {
            Ok(bundle) => Ok(bundle == self.stream.encode()?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(PortError::Unavailable(format!(
                "could not re-read committed memory bundle `{}`: {error}",
                path.display()
            ))),
        }
    }
}

async fn tail_at(store: &EmbeddedKernelStore, position: u64) -> Result<LogTail, PortError> {
    store
        .read_points(move |reads| {
            let last = reads.last_event_sequence()?;
            if last != position {
                return Ok(LogTail::of(last, None));
            }
            Ok(LogTail::of(last, reads.event(last)?.as_ref()))
        })
        .await
}
