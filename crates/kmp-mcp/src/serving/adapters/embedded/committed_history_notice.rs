use std::sync::Mutex;
use std::time::SystemTime;

use kmp_embedded::{CommitNativeBundle, EmbeddedKernelStore, HistoryRelation, OpenReconciliation};
use serde_json::Value;

/// What a project session says on its reads while its store and committed
/// bundle disagree, kept current with the committed file.
///
/// Reads used to answer from a store that disagreed with the repository and
/// say nothing; only the next refused write named the bundle. The store is
/// reconciled when the session opens, and again before a read whenever the
/// committed file changed since (a pull, or someone resolving a divergence
/// in another terminal) or the last attempt found another writer holding the
/// lock. A state reconciliation resolves needs no notice; one it leaves alone
/// is carried, short, on every read until it is resolved. The full advice
/// stays in the write refusal and in `doctor`.
#[derive(Debug, Default)]
pub(crate) struct CommittedHistoryNotice(Mutex<NoticeState>);

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct NoticeState {
    notice: Option<String>,
    /// The committed file as of the last reconciliation or guarded write.
    seen: Option<(u64, SystemTime)>,
    /// The last attempt could not run (lock held, or an error): try again.
    retry: bool,
}

impl CommittedHistoryNotice {
    /// Reconciles as the session opens.
    pub(crate) fn on_open(bundle: &CommitNativeBundle, store: &EmbeddedKernelStore) -> Self {
        Self(Mutex::new(reconcile(bundle, store)))
    }

    /// Before a read: reconciles again when the committed file moved or the
    /// last attempt could not run. A stat when nothing changed.
    pub(crate) fn refresh(&self, bundle: &CommitNativeBundle, store: &EmbeddedKernelStore) {
        let Ok(mut state) = self.0.lock() else {
            return;
        };
        if state.retry || state.seen != stamp(bundle) {
            *state = reconcile(bundle, store);
        }
    }

    /// After a write tool succeeded on a store that was in step: the guard
    /// proved the histories equal and published, so the file this session
    /// just wrote is no news. A store that was not in step keeps its notice
    /// and its stamp — a tool that wrote nothing (a review, a dry run) proves
    /// nothing — and the next read reconciles if the file moved.
    pub(crate) fn note_published(&self, bundle: &CommitNativeBundle) {
        if let Ok(mut state) = self.0.lock()
            && state.notice.is_none()
            && !state.retry
        {
            state.seen = stamp(bundle);
        }
    }

    /// Adds the notice to a read's warnings and to its text.
    pub(crate) fn qualify(&self, tool: &str, mut result: Value) -> Value {
        let notice = match self.0.lock() {
            Ok(state) => state.notice.clone(),
            Err(_) => None,
        };
        let Some(notice) = notice else {
            return result;
        };
        if !matches!(
            tool,
            "kmp_wake" | "kmp_ask" | "kmp_time" | "kmp_relate" | "kmp_trace" | "kmp_inspect"
        ) {
            return result;
        }
        if let Some(content) = result
            .get_mut("structuredContent")
            .and_then(Value::as_object_mut)
        {
            let warnings = content
                .entry("warnings")
                .or_insert_with(|| Value::Array(Vec::new()));
            if let Some(warnings) = warnings.as_array_mut() {
                warnings.insert(0, Value::String(notice.clone()));
            }
        }
        if let Some(text) = result
            .pointer_mut("/content/0/text")
            .and_then(|text| text.as_str().map(str::to_string))
        {
            result["content"][0]["text"] = Value::String(noted(&text, &notice));
        }
        result
    }
}

/// The note goes after an incomplete-read instruction, which must stay the
/// first thing a host shows.
fn noted(text: &str, notice: &str) -> String {
    match text.split_once('\n') {
        Some((first, rest)) if first.starts_with("READ_INCOMPLETE:") => {
            format!("{first}\nNOTE: {notice}\n{rest}")
        }
        _ => format!("NOTE: {notice}\n{text}"),
    }
}

fn stamp(bundle: &CommitNativeBundle) -> Option<(u64, SystemTime)> {
    let metadata = std::fs::metadata(bundle.path()).ok()?;
    Some((metadata.len(), metadata.modified().ok()?))
}

fn reconcile(bundle: &CommitNativeBundle, store: &EmbeddedKernelStore) -> NoticeState {
    // Stat after reconciling: a recovery rewrites the file, and that write
    // is this session's own, not news for the next read.
    let outcome = bundle.reconcile_on_open(store);
    let seen = stamp(bundle);
    let path = bundle.path().display();
    let (notice, retry) = match outcome {
        Ok(OpenReconciliation::Current) => (None, false),
        Ok(OpenReconciliation::Busy) => (None, true),
        Ok(OpenReconciliation::FastForwarded { appended, events }) => {
            tracing::info!(bundle = %path, appended, events,
                "caught the project store up with its committed memory");
            (None, false)
        }
        Ok(OpenReconciliation::Recovered { events }) => {
            tracing::info!(bundle = %path, events,
                "published an interrupted write to the committed memory");
            (None, false)
        }
        Ok(OpenReconciliation::StoreAhead(relation)) => (
            Some(format!(
                "This store holds {} writes the committed memory `{path}` does not; writes are \
                 refused until `kmp-mcp export` checkpoints them.",
                relation.store_events() - relation.bundle_events()
            )),
            false,
        ),
        Ok(OpenReconciliation::Diverged(relation)) => {
            let position = match relation {
                HistoryRelation::Diverged { position, .. } => position,
                _ => 0,
            };
            (
                Some(format!(
                    "The committed memory `{path}` and this store are different histories \
                     (first difference at event {position}): these reads answer from the store \
                     only and writes are refused until one side is kept; `kmp-mcp doctor` names \
                     both ways out."
                )),
                false,
            )
        }
        Ok(OpenReconciliation::BundleMissing { store_events }) => (
            Some(format!(
                "This store holds {store_events} events but the committed memory `{path}` is \
                 missing; writes are refused until `kmp-mcp export` writes it."
            )),
            false,
        ),
        Err(error) => (
            Some(format!(
                "This store could not be compared with the committed memory `{path}`: {error}. \
                 These reads may not include what the repository holds."
            )),
            true,
        ),
    };
    if let Some(notice) = &notice {
        tracing::warn!(bundle = %path, %notice, "committed memory notice");
    }
    NoticeState {
        notice,
        seen,
        retry,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn notice(text: &str) -> CommittedHistoryNotice {
        CommittedHistoryNotice(Mutex::new(NoticeState {
            notice: Some(text.to_string()),
            seen: None,
            retry: false,
        }))
    }

    #[test]
    fn a_notice_reaches_the_warnings_and_the_text_of_reads_only() {
        let notice = notice("bundle diverged");
        let read = notice.qualify(
            "kmp_ask",
            json!({"content":[{"type":"text","text":"answer"}],"structuredContent":{"answer":"x"}}),
        );
        assert_eq!(read["structuredContent"]["warnings"][0], "bundle diverged");
        assert!(
            read["content"][0]["text"]
                .as_str()
                .is_some_and(|text| text.starts_with("NOTE: bundle diverged"))
        );

        let write = notice.qualify(
            "kmp_ingest",
            json!({"content":[{"type":"text","text":"ok"}],"structuredContent":{}}),
        );
        assert!(write["structuredContent"].get("warnings").is_none());
        assert_eq!(
            CommittedHistoryNotice::default().qualify("kmp_ask", json!({"a": 1})),
            json!({"a": 1})
        );
    }

    #[test]
    fn an_incomplete_read_instruction_stays_first() {
        assert_eq!(
            noted("READ_INCOMPLETE: finish it.\nsummary", "diverged"),
            "READ_INCOMPLETE: finish it.\nNOTE: diverged\nsummary"
        );
        assert_eq!(noted("summary", "diverged"), "NOTE: diverged\nsummary");
    }
}
