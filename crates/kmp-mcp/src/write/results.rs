use serde_json::{Value, json};

use super::plan::KernelWritePlan;

pub(crate) fn write_dry_run_result(plan: &KernelWritePlan) -> Value {
    json!({
        "accepted": false,
        "dry_run": true,
        "summary": write_summary(plan),
        "generated_refs": plan.generated_refs,
        "relations": plan.relations,
        "relation_quality": plan.relation_quality,
        "relation_quality_metrics": plan.relation_quality_metrics,
        "ingest_preview": plan.ingest_arguments,
        "diagnostics": plan.diagnostics,
        "next_suggested_reads": plan.next_suggested_reads
    })
}

pub(crate) fn write_commit_result(
    plan: &KernelWritePlan,
    ingest_result: Value,
    viewer_url: Option<&str>,
    orphaned_bundle: Option<&kmp_embedded::OrphanedProjectBundle>,
) -> Value {
    let mut result = json!({
        "accepted": true,
        "dry_run": false,
        "summary": write_summary(plan),
        "generated_refs": plan.generated_refs,
        "relations": plan.relations,
        "relation_quality": plan.relation_quality,
        "relation_quality_metrics": plan.relation_quality_metrics,
        "ingest_result": ingest_result,
        "diagnostics": plan.diagnostics,
        "next_suggested_reads": plan.next_suggested_reads
    });
    if let Some(url) = viewer_url {
        result["viewer"] = viewer_invitation(url);
    }
    if let Some(orphaned) = orphaned_bundle {
        result["durability"] = orphaned_bundle_notice(orphaned);
    }
    result
}

pub(super) fn orphaned_bundle_notice(orphaned: &kmp_embedded::OrphanedProjectBundle) -> Value {
    json!({
        "bundle_orphaned": true,
        "bundle_path": orphaned.bundle_path.display().to_string(),
        "project_store_path": orphaned.project_store_path.display().to_string(),
        "selected_store_path": orphaned.selected_store_path.display().to_string(),
        "reason": orphaned.reason,
        "tell_the_user": format!(
            "This project write succeeded in `{}`, but `{}` is no longer maintained because \
             the project store `{}` could not be selected: {}. Say this once; refresh only this \
             project's memory with `kmp-mcp export {} --about <project-about>` and inspect the \
             diff.",
            orphaned.selected_store_path.display(),
            orphaned.bundle_path.display(),
            orphaned.project_store_path.display(),
            orphaned.reason,
            orphaned.bundle_path.display(),
        )
    })
}

/// The one moment a link to the viewer is worth spending a line on: memory
/// that did not exist a second ago now does, and there is somewhere to see
/// it. Phrased for the human, because the agent is only carrying it.
pub(super) fn viewer_invitation(url: &str) -> Value {
    json!({
        "url": url,
        "tell_the_user": format!(
            "Their memory is now a graph they can open: {url} — already running, \
             loopback only, read-only, and protected by a one-session capability. \
             Say it once; it is the same link all session."
        )
    })
}

pub(super) fn write_summary(plan: &KernelWritePlan) -> String {
    let memory = &plan.ingest_arguments["memory"];
    let entry_count = memory["entries"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default();
    let relation_count = memory["relations"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default();
    let evidence_count = memory["evidence"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default();
    format!(
        "Prepared {entry_count} {}, {relation_count} {}, and {evidence_count} {} for {}.",
        plural(entry_count, "entry", "entries"),
        plural(relation_count, "relation", "relations"),
        plural(evidence_count, "evidence item", "evidence items"),
        plan.about
    )
}

pub(super) fn plural<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 { singular } else { plural }
}
