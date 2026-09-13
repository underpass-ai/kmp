use kmp_mcp::guide;
use kmp_mcp::summaries::{AuditScope, SummaryAudit};

use super::pending_summary::PendingSummary;

use super::{looks_like_option, unknown_option};

/// `kmp-mcp summaries pending [<about>…] [--json]`: the memories in the
/// selected store that owe an English search summary.
///
/// The kernel cannot write the summary; the agent does, with
/// `kmp_write_memory` and `search_summaries`. This is the list it
/// works from. It is a projection of the one audit `kmp_summaries_audit`
/// returns and the doctor counts — the debt half of it, rendered for a
/// terminal. The audit's weakness signals are for the agent that can act on
/// them; what a terminal needs is the list of memories a question cannot
/// reach at all.
pub(super) async fn run_summaries_command(args: &[&str]) -> i32 {
    let Some((verb, rest)) = args.split_first() else {
        eprintln!("kmp-mcp: summaries takes `pending [<about>…] [--json]`");
        return 2;
    };
    if *verb != "pending" {
        eprintln!("kmp-mcp: summaries takes `pending [<about>…] [--json]`, not `{verb}`");
        return 2;
    }
    let mut abouts: Vec<String> = Vec::new();
    let mut json = false;
    for argument in rest {
        match *argument {
            "--json" => json = true,
            other if looks_like_option(other) => return unknown_option("summaries", other),
            other => abouts.push(other.to_string()),
        }
    }
    let scope = match abouts.len() {
        0 => AuditScope::AllAbouts,
        1 => AuditScope::CurrentAbout(abouts[0].clone()),
        _ => AuditScope::Abouts(abouts.clone()),
    };

    let resolved = match kmp_embedded::resolve_data_dir_from_env() {
        Ok(resolved) => resolved,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let bundle = kmp_embedded::EmbeddedKernelStore::open(resolved.path())
        .and_then(|store| store.export_bundle_excluding_abouts_blocking(&guide::abouts_owned()));
    let bundle = match bundle {
        Ok(bundle) => bundle,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let pending = match SummaryAudit::read(&bundle, &scope) {
        Ok(audit) => audit.owed().map(PendingSummary::of).collect::<Vec<_>>(),
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };

    if json {
        match serde_json::to_string_pretty(&pending) {
            Ok(rendered) => println!("{rendered}"),
            Err(error) => {
                eprintln!("kmp-mcp: could not render the list: {error}");
                return 2;
            }
        }
        return 0;
    }
    if pending.is_empty() {
        println!(
            "every memory{} that needs an English search summary carries one",
            selected(&abouts)
        );
        return 0;
    }
    println!(
        "{} {} an English search summary. Render each in plain English, keep every number, \
         identifier and acronym exactly as written, and attach it with kmp_write_memory \
         (search_summaries with ref and summary_en).\n",
        pending.len(),
        if pending.len() == 1 {
            "memory owes"
        } else {
            "memories owe"
        }
    );
    let mut current_about = None;
    for item in &pending {
        if current_about != Some(item.about.as_str()) {
            println!("{}", item.about);
            current_about = Some(item.about.as_str());
        }
        println!("  {} [{}]", item.reference, item.kind);
        println!("    {}", item.text);
        if !item.faults.is_empty() {
            println!("    summary_en will not carry: {}", item.faults.join("; "));
        }
    }
    0
}

/// How the empty answer names what was read. No about named is the whole
/// store, and reads as it always did.
fn selected(abouts: &[String]) -> String {
    match abouts {
        [] => String::new(),
        [one] => format!(" in `{one}`"),
        many => format!(
            " in {}",
            many.iter()
                .map(|about| format!("`{about}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}
