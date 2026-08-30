use std::path::Path;

use kmp_embedded::{OrphanedProjectBundle, ResolvedDataDir};

use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;

use super::embedded_memory_probe::store_file_on_disk;

/// Whether the machine state has a current, verifiable git-native copy. The
/// store itself is never opened: the pending marker brackets every write, and
/// file modification times catch stores written by older binaries that did
/// not create one.
pub(crate) fn committed_bundle_finding(resolved: &ResolvedDataDir) -> Option<LifecycleFinding> {
    if let Some(orphaned) = resolved.orphaned_bundle() {
        return Some(orphaned_bundle_finding(orphaned));
    }
    let bundle = kmp_embedded::project_bundle_path(resolved)?;
    let store = store_file_on_disk(resolved.path());
    let pending = kmp_embedded::pending_bundle_exports(resolved.path());
    if !pending.is_empty() {
        let mut finding = LifecycleFinding::new(
            DiagnosticSeverity::Fail,
            format!(
                "{} write {} not proved in the committed bundle",
                pending.len(),
                if pending.len() == 1 { "is" } else { "are" }
            ),
        )
        .with_detail(format!("bundle: {}", bundle.display()))
        .with_detail(
            "the store may contain a write whose process stopped before export completed; stop \
             other KMP sessions, run `kmp-mcp export`, inspect the diff, then run `kmp-mcp \
             export --repair-pending` and commit it",
        );
        for marker in pending {
            finding = finding.with_detail(format!("pending: {}", marker.display()));
        }
        return Some(finding);
    }

    if !bundle.exists() {
        return Some(if store.is_some() {
            LifecycleFinding::new(
                DiagnosticSeverity::Fail,
                "memory exists only in the gitignored store",
            )
            .with_detail(format!("missing: {}", bundle.display()))
            .with_detail("run `kmp-mcp export`, inspect the diff, and commit it")
        } else {
            LifecycleFinding::new(DiagnosticSeverity::Ok, "no memory to protect yet").with_detail(
                format!("the first write will maintain {}", bundle.display()),
            )
        });
    }

    let text = match std::fs::read_to_string(&bundle) {
        Ok(text) => text,
        Err(error) => {
            return Some(
                LifecycleFinding::new(
                    DiagnosticSeverity::Fail,
                    "the committed memory cannot be read",
                )
                .with_detail(format!("{}: {error}", bundle.display())),
            );
        }
    };
    let header = match kmp_embedded::verify_bundle(&text) {
        Ok(header) => header,
        Err(error) => {
            return Some(
                LifecycleFinding::new(
                    DiagnosticSeverity::Fail,
                    "the committed memory does not verify",
                )
                .with_detail(format!("{}: {error}", bundle.display()))
                .with_detail("do not restore it; regenerate with `kmp-mcp export` first"),
            );
        }
    };
    if store.is_some() {
        let live = kmp_embedded::EmbeddedKernelStore::open(resolved.path())
            .and_then(|store| store.export_bundle_blocking());
        let live = match live {
            Ok(live) => live,
            Err(error) => {
                return Some(
                    LifecycleFinding::new(
                        DiagnosticSeverity::Fail,
                        "the live memory cannot be audited",
                    )
                    .with_detail(error.to_string()),
                );
            }
        };
        let live_header = match kmp_embedded::verify_bundle(&live) {
            Ok(header) => header,
            Err(error) => {
                return Some(
                    LifecycleFinding::new(
                        DiagnosticSeverity::Fail,
                        "the live memory export does not verify",
                    )
                    .with_detail(error.to_string()),
                );
            }
        };
        if let Err(error) = kmp_embedded::merge_bundles(&text, &live, "doctor-history-audit") {
            return Some(
                LifecycleFinding::new(
                    DiagnosticSeverity::Fail,
                    "the live store and committed memory are divergent histories",
                )
                .with_detail(error.to_string())
                .with_detail(
                    "reconcile them explicitly; do not restore an archived machine history",
                ),
            );
        }
        if header.event_count != live_header.event_count
            || header.content_digest != live_header.content_digest
        {
            return Some(
                LifecycleFinding::new(
                    DiagnosticSeverity::Fail,
                    "the live store and committed memory are different revisions",
                )
                .with_detail(format!(
                    "live events: {}; committed events: {}",
                    live_header.event_count, header.event_count
                ))
                .with_detail("run `kmp-mcp export`, inspect the diff, and reconcile explicitly"),
            );
        }
    }
    if let Some(store) = store
        && newer_than(&store, &bundle)
    {
        return Some(
            LifecycleFinding::new(
                DiagnosticSeverity::Fail,
                "the gitignored store is newer than its committed memory",
            )
            .with_detail(format!("store:  {}", store.display()))
            .with_detail(format!("bundle: {}", bundle.display()))
            .with_detail("run `kmp-mcp export`, inspect the diff, and commit it"),
        );
    }
    if header.bundle_format < kmp_embedded::BUNDLE_FORMAT_VERSION {
        return Some(
            LifecycleFinding::new(
                DiagnosticSeverity::Warn,
                "the committed memory uses a legacy bundle header",
            )
            .with_detail(format!(
                "{} events · no snapshot identity or digest",
                header.event_count
            ))
            .with_detail("run `kmp-mcp export` to upgrade it without changing the events"),
        );
    }

    Some(
        LifecycleFinding::new(
            DiagnosticSeverity::Ok,
            format!(
                "snapshot {} protects {} {}",
                header.snapshot_id,
                header.event_count,
                if header.event_count == 1 {
                    "event"
                } else {
                    "events"
                }
            ),
        )
        .with_detail(format!("bundle: {}", bundle.display()))
        .with_detail(format!("digest: {}", header.content_digest))
        .with_detail(format!("abouts: {}", header.abouts.join(" "))),
    )
}

fn orphaned_bundle_finding(orphaned: &OrphanedProjectBundle) -> LifecycleFinding {
    let mut finding = LifecycleFinding::new(
        DiagnosticSeverity::Fail,
        "this project's committed memory is no longer being maintained",
    );
    let bundle_detail = match std::fs::read_to_string(&orphaned.bundle_path) {
        Ok(bundle) => match kmp_embedded::verify_bundle(&bundle) {
            Ok(header) => format!(
                "bundle: {} (last event {}, snapshot time {} ms)",
                orphaned.bundle_path.display(),
                header.event_count,
                header.created_at_unix_ms
            ),
            Err(error) => format!(
                "bundle: {} (cannot verify: {error})",
                orphaned.bundle_path.display()
            ),
        },
        Err(error) => format!(
            "bundle: {} (cannot read: {error})",
            orphaned.bundle_path.display()
        ),
    };
    finding = finding
        .with_detail(bundle_detail)
        .with_detail(format!(
            "project store: {} — not selected: {}",
            orphaned.project_store_path.display(),
            orphaned.reason
        ))
        .with_detail(format!(
            "writes are going to: {}",
            orphaned.selected_store_path.display()
        ));

    let abouts = std::fs::read_to_string(&orphaned.bundle_path)
        .ok()
        .and_then(|bundle| kmp_embedded::verify_bundle(&bundle).ok())
        .map(|header| header.abouts)
        .unwrap_or_default();
    if let [about] = abouts.as_slice() {
        finding.with_detail(format!(
            "automatic maintenance resumes only when the project store opens again; until then, \
             refresh this bundle explicitly with `kmp-mcp export {} --about {about}`",
            orphaned.bundle_path.display()
        ))
    } else {
        finding.with_detail(
            "automatic maintenance resumes only when the project store opens again; a filtered \
             explicit export can refresh known abouts safely",
        )
    }
}

fn newer_than(left: &Path, right: &Path) -> bool {
    let modified = |path: &Path| std::fs::metadata(path).and_then(|metadata| metadata.modified());
    matches!((modified(left), modified(right)), (Ok(left), Ok(right)) if left > right)
}

#[cfg(test)]
mod tests {
    use super::committed_bundle_finding;
    use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
    use kmp_embedded::{OrphanedProjectBundle, ResolvedDataDir};

    #[test]
    fn a_project_store_without_a_committed_copy_is_a_durability_failure() {
        let project = tempfile::tempdir().expect("project");
        let data_dir = project.path().join(".kernel");
        let store = data_dir.join("store");
        std::fs::create_dir_all(&store).expect("store dir");
        std::fs::write(store.join("retired-layout.bin"), b"store").expect("store marker");
        let resolved = ResolvedDataDir::Project(data_dir);

        let finding = committed_bundle_finding(&resolved).expect("project finding");
        assert_eq!(finding.severity(), DiagnosticSeverity::Fail);
        assert!(finding.headline().contains("only in the gitignored store"));
        assert!(
            finding
                .detail()
                .iter()
                .any(|line| line.contains("kmp-mcp export"))
        );
    }
    #[test]
    fn a_pending_write_is_louder_than_an_existing_bundle() {
        let project = tempfile::tempdir().expect("project");
        let data_dir = project.path().join(".kernel");
        let pending = data_dir.join(kmp_embedded::PENDING_EXPORT_DIR);
        std::fs::create_dir_all(&pending).expect("pending dir");
        std::fs::write(pending.join("write.pending"), b"pending").expect("marker");
        let resolved = ResolvedDataDir::Project(data_dir);

        let finding = committed_bundle_finding(&resolved).expect("project finding");
        assert_eq!(finding.severity(), DiagnosticSeverity::Fail);
        assert!(finding.headline().contains("not proved"));
        assert!(
            finding
                .detail()
                .iter()
                .any(|line| line.contains("pending:"))
        );
    }
    #[test]
    fn a_legacy_bundle_is_readable_but_not_mistaken_for_an_identified_snapshot() {
        let project = tempfile::tempdir().expect("project");
        let data_dir = project.path().join(".kernel");
        let bundle = project.path().join(".kmp/memory.jsonl");
        std::fs::create_dir_all(bundle.parent().expect("parent")).expect("bundle dir");
        std::fs::write(
            bundle,
            r#"{"bundle_format":1,"store_format":1,"event_count":0,"kernel_version":"0.1.3"}"#,
        )
        .expect("legacy bundle");
        let resolved = ResolvedDataDir::Project(data_dir);

        let finding = committed_bundle_finding(&resolved).expect("project finding");
        assert_eq!(finding.severity(), DiagnosticSeverity::Warn);
        assert!(finding.headline().contains("legacy"));
    }
    #[test]
    fn an_orphaned_project_bundle_is_compared_with_the_store_that_receives_writes() {
        let project = tempfile::tempdir().expect("project");
        let bundle = project.path().join(".kmp/memory.jsonl");
        std::fs::create_dir_all(bundle.parent().expect("bundle parent")).expect("bundle dir");
        std::fs::write(
            &bundle,
            r#"{"bundle_format":1,"store_format":1,"event_count":0,"kernel_version":"0.2.4"}"#,
        )
        .expect("legacy bundle");
        let project_store = project.path().join(".kernel");
        let selected_store = project.path().join("user-store");
        let resolved = ResolvedDataDir::UserFallback {
            path: selected_store.clone(),
            orphaned_bundle: OrphanedProjectBundle {
                bundle_path: bundle.clone(),
                project_store_path: project_store.clone(),
                selected_store_path: selected_store.clone(),
                reason: "store format 1 is retired".to_string(),
            },
        };

        let finding = committed_bundle_finding(&resolved).expect("orphan finding");
        assert_eq!(finding.severity(), DiagnosticSeverity::Fail);
        assert!(finding.headline().contains("no longer being maintained"));
        assert!(
            finding
                .detail()
                .iter()
                .any(|line| line.contains(&bundle.display().to_string()))
        );
        assert!(
            finding
                .detail()
                .iter()
                .any(|line| line.contains(&project_store.display().to_string())
                    && line.contains("not selected"))
        );
        assert!(finding.detail().iter().any(|line| {
            line.contains("writes are going to")
                && line.contains(&selected_store.display().to_string())
        }));
    }
}
