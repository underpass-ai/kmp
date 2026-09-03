use kmp_embedded::ResolvedDataDir;

use crate::guide::domain::shipped_guide_abouts::ShippedGuideAbouts;
use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;
use crate::summaries::pending;

/// How many memories in the selected store still owe an English search
/// summary, read off the store's own event log.
///
/// A store written before summaries existed answers English questions only
/// through the bridge table, if one is installed. Saying so here is the
/// first step of a store that can attest what it demands of its writers; a
/// count that is not zero is a warning, never a failure, because the memory
/// is intact and the fix is a write.
pub(crate) fn search_summary_finding(resolved: &ResolvedDataDir) -> Option<LifecycleFinding> {
    if !kmp_embedded::store_file_path_for(resolved.path(), kmp_embedded::StorageEngine::Sqlite)
        .exists()
    {
        return None;
    }
    let bundle = kmp_embedded::EmbeddedKernelStore::open(resolved.path()).and_then(|store| {
        store.export_bundle_excluding_abouts_blocking(&ShippedGuideAbouts::owned())
    });
    let bundle = match bundle {
        Ok(bundle) => bundle,
        Err(error) => {
            return Some(
                LifecycleFinding::new(
                    DiagnosticSeverity::Warn,
                    "search summaries could not be audited",
                )
                .with_detail(error.to_string()),
            );
        }
    };
    let pending = match pending(&bundle, None) {
        Ok(pending) => pending,
        Err(error) => {
            return Some(
                LifecycleFinding::new(
                    DiagnosticSeverity::Warn,
                    "search summaries could not be audited",
                )
                .with_detail(error),
            );
        }
    };
    if pending.is_empty() {
        return Some(LifecycleFinding::new(
            DiagnosticSeverity::Ok,
            "search summaries: every memory that needs one carries one",
        ));
    }
    let abouts = pending
        .iter()
        .map(|item| item.about.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let faulty = pending
        .iter()
        .filter(|item| !item.faults.is_empty())
        .count();
    Some(
        LifecycleFinding::new(
            DiagnosticSeverity::Warn,
            format!(
                "search summaries: {} {} one, across {} {}",
                pending.len(),
                if pending.len() == 1 {
                    "memory owes"
                } else {
                    "memories owe"
                },
                abouts.len(),
                if abouts.len() == 1 { "about" } else { "abouts" }
            ),
        )
        .with_detail(format!(
            "{} with no summary, {faulty} with one the lint refuses",
            pending.len() - faulty
        ))
        .with_detail(
            "list them with `kmp-mcp summaries pending`; the agent attaches each with \
             kmp_write_memory, intent record_summary",
        ),
    )
}
