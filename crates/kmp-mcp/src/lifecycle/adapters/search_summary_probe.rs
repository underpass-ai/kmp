use kmp_embedded::ResolvedDataDir;

use crate::guide;
use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;
use crate::summaries::{AuditScope, SummaryAudit};

/// How many memories in the selected store still owe an English search
/// summary, read off the store's own event log.
///
/// A store written before summaries existed answers English questions only
/// through the bridge table, if one is installed. Saying so here is the
/// first step of a store that can attest what it demands of its writers; a
/// count that is not zero is a warning, never a failure, because the memory
/// is intact and the fix is a write.
///
/// The count comes from [`SummaryAudit`], the same reading `kmp_summaries_audit`
/// returns to an agent and `kmp-mcp summaries pending` prints. There is no
/// second counter here to drift away from it.
pub(crate) fn search_summary_finding(resolved: &ResolvedDataDir) -> Option<LifecycleFinding> {
    if !kmp_embedded::store_file_path_for(resolved.path(), kmp_embedded::StorageEngine::Sqlite)
        .exists()
    {
        return None;
    }
    let bundle = kmp_embedded::EmbeddedKernelStore::open(resolved.path())
        .and_then(|store| store.export_bundle_excluding_abouts_blocking(&guide::abouts_owned()));
    let bundle = match bundle {
        Ok(bundle) => bundle,
        Err(error) => return Some(unreadable(error.to_string())),
    };
    let audit = match SummaryAudit::read(&bundle, &AuditScope::AllAbouts) {
        Ok(audit) => audit,
        Err(error) => return Some(unreadable(error)),
    };
    let totals = audit.totals();
    let weak = totals.weak;
    if totals.owed() == 0 {
        let finding = LifecycleFinding::new(
            DiagnosticSeverity::Ok,
            "search summaries: every memory that needs one carries one",
        );
        return Some(if weak == 0 {
            finding
        } else {
            finding.with_detail(weak_detail(weak))
        });
    }
    let abouts = audit
        .totals_by_about()
        .iter()
        .filter(|(_, totals)| totals.owed() > 0)
        .count();
    let mut finding = LifecycleFinding::new(
        DiagnosticSeverity::Warn,
        format!(
            "search summaries: {} {} one, across {} {}",
            totals.owed(),
            if totals.owed() == 1 {
                "memory owes"
            } else {
                "memories owe"
            },
            abouts,
            if abouts == 1 { "about" } else { "abouts" }
        ),
    )
    .with_detail(format!(
        "{} with no summary, {} with one the lint refuses",
        totals.missing, totals.refused
    ));
    if weak > 0 {
        finding = finding.with_detail(weak_detail(weak));
    }
    Some(finding.with_detail(
        "list them with `kmp-mcp summaries pending`, or read the whole audit with the \
         kmp_summaries_audit tool; the agent attaches each with kmp_write_memory, \
         search_summaries",
    ))
}

/// A summary that stands and still retrieves little is worth saying, and is
/// never a debt: it is reported beside the count, not inside it.
fn weak_detail(weak: usize) -> String {
    format!(
        "{weak} {} the lint accepts but that carr{} little retrieval; read kmp_summaries_audit \
         for the reasons",
        if weak == 1 { "summary" } else { "summaries" },
        if weak == 1 { "ies" } else { "y" }
    )
}

fn unreadable(detail: String) -> LifecycleFinding {
    LifecycleFinding::new(
        DiagnosticSeverity::Warn,
        "search summaries could not be audited",
    )
    .with_detail(detail)
}
