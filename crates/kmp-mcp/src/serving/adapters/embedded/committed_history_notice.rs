use kmp_embedded::{
    CommitNativeBundle, EmbeddedKernelStore, OpenReconciliation, diverged_advice,
    store_ahead_advice,
};
use serde_json::Value;

/// What a project session says on its reads when its store and committed
/// bundle could not be brought together on open.
///
/// Reads used to answer from a store that disagreed with the repository and
/// say nothing; only the next refused write named the bundle. A state that
/// open resolved (caught up, recovered) needs no notice; one it left alone
/// is carried on every read until the session ends.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CommittedHistoryNotice(Option<String>);

impl CommittedHistoryNotice {
    /// Reconciles `bundle` with `store` as the session opens, and keeps what
    /// the reads must say about it.
    pub(crate) fn on_open(bundle: &CommitNativeBundle, store: &EmbeddedKernelStore) -> Self {
        let outcome = bundle.reconcile_on_open(store);
        let notice = match outcome {
            Ok(OpenReconciliation::Current | OpenReconciliation::Busy) => None,
            Ok(OpenReconciliation::FastForwarded { appended, events }) => {
                tracing::info!(
                    bundle = %bundle.path().display(),
                    appended,
                    events,
                    "caught the project store up with its committed memory"
                );
                None
            }
            Ok(OpenReconciliation::Recovered { events }) => {
                tracing::info!(
                    bundle = %bundle.path().display(),
                    events,
                    "published an interrupted write to the committed memory"
                );
                None
            }
            Ok(OpenReconciliation::StoreAhead(relation)) => Some(format!(
                "Writes are refused until this is resolved: {}.",
                store_ahead_advice(bundle.path(), &relation)
            )),
            Ok(OpenReconciliation::Diverged(relation)) => Some(format!(
                "These reads answer from the live store only, and writes are refused: {}",
                diverged_advice(bundle.path(), bundle.data_dir(), &relation)
            )),
            Ok(OpenReconciliation::BundleMissing { store_events }) => Some(format!(
                "The live project store holds {store_events} events but the committed memory \
                 `{}` is missing; writes are refused until `kmp-mcp export` writes it.",
                bundle.path().display()
            )),
            Err(error) => Some(format!(
                "The live store could not be compared with the committed memory `{}`: {error}. \
                 These reads may not include what the repository holds.",
                bundle.path().display()
            )),
        };
        if let Some(notice) = &notice {
            tracing::warn!(bundle = %bundle.path().display(), %notice, "committed memory notice");
        }
        Self(notice)
    }

    /// Adds the notice to a read's warnings and to the front of its text.
    pub(crate) fn qualify(&self, tool: &str, mut result: Value) -> Value {
        let Some(notice) = &self.0 else {
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
            result["content"][0]["text"] = Value::String(format!("NOTE: {notice}\n{text}"));
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_notice_reaches_the_warnings_and_the_text_of_reads_only() {
        let notice = CommittedHistoryNotice(Some("bundle diverged".to_string()));
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
}
