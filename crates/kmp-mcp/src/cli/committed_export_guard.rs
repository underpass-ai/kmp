//! `export` never drops a committed event by accident.
//!
//! `.kmp/memory.jsonl` travels in git: a teammate's memory arrives in it with
//! a pull, before this store has seen it. Overwriting the file with this
//! store's history used to be what `doctor` advised in exactly that state,
//! and it removed the teammate's memory from the working tree. Any export
//! whose destination is a committed `.kmp/memory.jsonl` — with no file or a
//! file given in any form, from the project's own store or one pinned by
//! `KMP_MCP_DATA_DIR`, whole or filtered with `--about` — replaces it only
//! when every event it holds is kept, unless the person says otherwise with
//! `--overwrite-committed`.

use std::collections::BTreeMap;
use std::path::Path;

use kmp_embedded::{HistoryRelation, OVERWRITE_COMMITTED_FLAG};

/// What the export writes and where, as far as the guard needs to know.
pub(super) struct ExportOver<'a> {
    pub(super) path: &'a Path,
    pub(super) data_dir: &'a Path,
    pub(super) exported: &'a str,
    /// The `--about` filter, empty for a whole export.
    pub(super) abouts: &'a [String],
    /// The destination is this store's own head, which a whole export
    /// catches up with first.
    pub(super) is_project_head: bool,
}

/// Refuses to write the export over the committed bundle at its path when
/// the committed history is not a prefix of it. `Ok` when the file is
/// absent, kept whole, or `overwrite` was asked for.
pub(super) fn refuse_dropping_committed(
    export: &ExportOver<'_>,
    overwrite: bool,
) -> Result<(), String> {
    if overwrite {
        return Ok(());
    }
    let path = export.path;
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
    let local = kmp_mcp::local_abouts();
    let unverifiable = |error| {
        format!(
            "export refused; nothing was written. The committed memory `{}` does not verify \
             ({error}): it may come from a newer KMP. Pass {OVERWRITE_COMMITTED_FLAG} to \
             replace it anyway.",
            path.display()
        )
    };
    let authored =
        kmp_embedded::bundle_excluding_abouts(&committed, &local).map_err(unverifiable)?;
    // Both sides through the same authored-memory policy: an export to an
    // explicit path carries the shipped guide, the committed file never does.
    let exported = kmp_embedded::bundle_excluding_abouts(export.exported, &local)
        .map_err(|error| format!("export refused; nothing was written. {error}"))?;
    let relation = kmp_embedded::compare_bundle_histories(&exported, &authored)
        .map_err(|error| format!("export refused; nothing was written. {error}"))?;
    if relation.bundle_is_prefix_of_store() {
        return Ok(());
    }
    let dropped = kmp_embedded::committed_events_not_kept(&exported, &authored)
        .map_err(|error| format!("export refused; nothing was written. {error}"))?;
    let loss = describe(&dropped);
    if !export.abouts.is_empty() {
        return Err(format!(
            "export refused; nothing was written. `{}` is committed memory, and this export \
             keeps only {}: writing it there would drop {loss}. Export without --about to keep \
             them, or pass {OVERWRITE_COMMITTED_FLAG} to replace it anyway.",
            path.display(),
            quoted(export.abouts.iter())
        ));
    }
    match relation {
        HistoryRelation::Diverged { .. } if export.is_project_head => Err(format!(
            "export refused; nothing was written. {}",
            kmp_embedded::diverged_advice(path, export.data_dir, &relation)
        )),
        HistoryRelation::BundleAhead { .. } if export.is_project_head => Err(format!(
            "export refused; nothing was written. `{}` is committed memory, and writing this \
             export over it would drop {loss} only the committed file holds. A full export \
             catches the store up first; if another KMP session held the store, close it and \
             run `kmp-mcp export` again. To replace the file anyway, pass \
             {OVERWRITE_COMMITTED_FLAG}.",
            path.display()
        )),
        _ => Err(format!(
            "export refused; nothing was written. `{}` is committed memory, and writing this \
             export over it would drop {loss} the store `{}` does not hold: it is not this \
             project's own store, so export does not catch it up. Bring them in with \
             `kmp-mcp import --from {} --about <about>` and export again, or pass \
             {OVERWRITE_COMMITTED_FLAG} to replace it anyway.",
            path.display(),
            export.data_dir.display(),
            path.display()
        )),
    }
}

/// `3 events (2 of `project:b`, 1 of `project:c`)`.
fn describe(dropped: &BTreeMap<String, u64>) -> String {
    let total: u64 = dropped.values().sum();
    if total == 0 {
        return "the committed order of its events".to_string();
    }
    let parts = dropped
        .iter()
        .map(|(about, count)| format!("{count} of `{about}`"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{} ({parts})", super::transfer::events(total))
}

fn quoted<'a>(abouts: impl Iterator<Item = &'a String>) -> String {
    abouts
        .map(|about| format!("`{about}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle_of(dir: &Path, events: &[(&str, &str)]) -> String {
        use kmp_domain::{ContextEventStore, ContextUpdatedEvent};
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let store = kmp_embedded::EmbeddedKernelStore::open(dir).expect("store");
            let mut revisions = BTreeMap::<&str, u64>::new();
            for (about, hash) in events {
                let revision = revisions.entry(about).or_insert(0);
                *revision += 1;
                let event = ContextUpdatedEvent {
                    root_node_id: (*about).to_string(),
                    role: "agent".to_string(),
                    revision: *revision,
                    content_hash: (*hash).to_string(),
                    changes: Vec::new(),
                    idempotency_key: Some(format!("guard:{hash}")),
                    logical_digest: None,
                    requested_by: Some("export-guard-test".to_string()),
                    occurred_at: std::time::UNIX_EPOCH + std::time::Duration::from_secs(*revision),
                };
                store.append(event, *revision - 1).await.expect("append");
            }
            store.export_bundle().await.expect("export")
        })
    }

    fn over<'a>(
        path: &'a Path,
        data_dir: &'a Path,
        exported: &'a str,
        abouts: &'a [String],
        is_project_head: bool,
    ) -> ExportOver<'a> {
        ExportOver {
            path,
            data_dir,
            exported,
            abouts,
            is_project_head,
        }
    }

    const A: &str = "project:guard";

    #[test]
    fn a_committed_file_that_is_not_kept_whole_is_refused_unless_overwritten() {
        let teammate = tempfile::tempdir().expect("teammate");
        let mine = tempfile::tempdir().expect("mine");
        let committed = bundle_of(teammate.path(), &[(A, "one"), (A, "two"), (A, "three")]);
        let short = bundle_of(mine.path(), &[(A, "one"), (A, "two")]);
        let file = teammate.path().join("memory.jsonl");
        std::fs::write(&file, &committed).expect("committed");
        let absent = mine.path().join("absent");
        let check = |path: &Path, exported: &str, head: bool, overwrite: bool| {
            refuse_dropping_committed(&over(path, mine.path(), exported, &[], head), overwrite)
        };

        let refused = check(&file, &short, true, false).expect_err("a shrinking export");
        assert!(
            refused.contains("drop 1 event (1 of `project:guard`)"),
            "{refused}"
        );
        assert!(refused.contains("catches the store up"), "{refused}");
        assert!(refused.contains(OVERWRITE_COMMITTED_FLAG), "{refused}");
        let pinned = check(&file, &short, false, false).expect_err("not this store's head");
        assert!(pinned.contains("kmp-mcp import --from"), "{pinned}");
        assert!(check(&file, &short, true, true).is_ok());
        assert!(check(&file, &committed, true, false).is_ok());
        assert!(check(&absent, &short, true, false).is_ok());
    }

    #[test]
    fn an_about_filter_over_other_abouts_names_what_it_would_drop() {
        let teammate = tempfile::tempdir().expect("teammate");
        let mine = tempfile::tempdir().expect("mine");
        let committed = bundle_of(
            teammate.path(),
            &[(A, "a1"), ("project:b", "b1"), ("project:b", "b2")],
        );
        let file = teammate.path().join("memory.jsonl");
        std::fs::write(&file, &committed).expect("committed");
        let subset = bundle_of(mine.path(), &[(A, "a1")]);
        let named = [A.to_string()];

        let refused =
            refuse_dropping_committed(&over(&file, mine.path(), &subset, &named, true), false)
                .expect_err("dropping project:b");
        assert!(
            refused.contains("drop 2 events (2 of `project:b`)"),
            "{refused}"
        );
        assert!(refused.contains("keeps only `project:guard`"), "{refused}");
        assert!(refused.contains("without --about"), "{refused}");
        assert!(refused.contains(OVERWRITE_COMMITTED_FLAG), "{refused}");

        // The same filter over a file that holds only that about is a save.
        let only = teammate.path().join("only.jsonl");
        std::fs::write(&only, &subset).expect("only");
        let longer = bundle_of(
            tempfile::tempdir().expect("longer").path(),
            &[(A, "a1"), (A, "a2")],
        );
        assert!(
            refuse_dropping_committed(&over(&only, mine.path(), &longer, &named, false), false)
                .is_ok()
        );
    }
}
