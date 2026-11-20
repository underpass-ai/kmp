//! `export` to the project head never drops a committed event by accident.
//!
//! `.kmp/memory.jsonl` travels in git: a teammate's memory arrives in it with
//! a pull, before this store has seen it. Overwriting the file with this
//! store's history used to be what `doctor` advised in exactly that state,
//! and it removed the teammate's memory from the working tree. The file is
//! now replaced only when every event it holds is kept, unless the person
//! says otherwise with `--overwrite-committed`.

use std::path::Path;

use kmp_embedded::{HistoryRelation, OVERWRITE_COMMITTED_FLAG};

/// Refuses to write `exported` over the committed bundle at `path` when the
/// committed history is not a prefix of it. `Ok` when the file is absent,
/// kept whole, or `overwrite` was asked for.
pub(super) fn refuse_shrinking_committed(
    path: &Path,
    data_dir: &Path,
    exported: &str,
    overwrite: bool,
) -> Result<(), String> {
    if overwrite {
        return Ok(());
    }
    let committed = match std::fs::read_to_string(path) {
        Ok(committed) => committed,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "export refused; nothing was written. The committed memory `{}` cannot be read \
                 ({error}), so KMP cannot prove the export keeps it. Pass {OVERWRITE_COMMITTED_FLAG} \
                 to replace it anyway.",
                path.display()
            ));
        }
    };
    let authored = kmp_embedded::bundle_excluding_abouts(&committed, &kmp_mcp::local_abouts())
        .map_err(|error| {
            format!(
                "export refused; nothing was written. The committed memory `{}` does not verify \
                 ({error}): it may come from a newer KMP. Pass {OVERWRITE_COMMITTED_FLAG} to \
                 replace it anyway.",
                path.display()
            )
        })?;
    let relation = kmp_embedded::compare_bundle_histories(exported, &authored)
        .map_err(|error| format!("export refused; nothing was written. {error}"))?;
    match relation {
        HistoryRelation::Equal { .. } | HistoryRelation::StoreAhead { .. } => Ok(()),
        HistoryRelation::BundleAhead {
            store_events,
            bundle_events,
        } => Err(format!(
            "export refused; nothing was written. The committed memory `{}` holds {bundle_events} \
             events and this export {store_events}: writing it would drop the {} events only the \
             committed file holds. A full export catches the store up first; if another KMP \
             session held the store, close it and run `kmp-mcp export` again. To replace the \
             file anyway, pass {OVERWRITE_COMMITTED_FLAG}.",
            path.display(),
            bundle_events - store_events
        )),
        HistoryRelation::Diverged { .. } => Err(format!(
            "export refused; nothing was written. {}",
            kmp_embedded::diverged_advice(path, data_dir, &relation)
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle_of(dir: &Path, hashes: &[&str]) -> String {
        use kmp_domain::{ContextEventStore, ContextUpdatedEvent};
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let store = kmp_embedded::EmbeddedKernelStore::open(dir).expect("store");
            for (index, hash) in hashes.iter().enumerate() {
                let revision = index as u64 + 1;
                let event = ContextUpdatedEvent {
                    root_node_id: "project:guard".to_string(),
                    role: "agent".to_string(),
                    revision,
                    content_hash: (*hash).to_string(),
                    changes: Vec::new(),
                    idempotency_key: Some(format!("guard:{hash}")),
                    logical_digest: None,
                    requested_by: Some("export-guard-test".to_string()),
                    occurred_at: std::time::UNIX_EPOCH + std::time::Duration::from_secs(revision),
                };
                store.append(event, revision - 1).await.expect("append");
            }
            store.export_bundle().await.expect("export")
        })
    }

    #[test]
    fn a_committed_file_that_is_not_kept_whole_is_refused_unless_overwritten() {
        let teammate = tempfile::tempdir().expect("teammate");
        let mine = tempfile::tempdir().expect("mine");
        let committed = bundle_of(teammate.path(), &["one", "two", "three"]);
        let short = bundle_of(mine.path(), &["one", "two"]);
        let file = teammate.path().join("memory.jsonl");
        std::fs::write(&file, &committed).expect("committed");

        let refused = refuse_shrinking_committed(&file, mine.path(), &short, false)
            .expect_err("a shrinking export is refused");
        assert!(refused.contains("drop the 1 events"), "{refused}");
        assert!(refused.contains(OVERWRITE_COMMITTED_FLAG), "{refused}");
        assert!(refuse_shrinking_committed(&file, mine.path(), &short, true).is_ok());
        assert!(refuse_shrinking_committed(&file, mine.path(), &committed, false).is_ok());
        assert!(
            refuse_shrinking_committed(&mine.path().join("absent"), mine.path(), &short, false)
                .is_ok()
        );
    }
}
