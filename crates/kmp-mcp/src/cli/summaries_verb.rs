use kmp_mcp::guide::domain::shipped_guide_abouts::ShippedGuideAbouts;
use kmp_mcp::summaries::pending;

use super::{looks_like_option, unknown_option};

/// `kmp-mcp summaries pending [<about>] [--json]`: the memories in the
/// selected store that owe an English search summary.
///
/// The kernel cannot write the summary; the agent does, with
/// `kmp_write_memory` and the intent `record_summary`. This is the list it
/// works from, and the doctor's count comes from the same reading.
pub(super) async fn run_summaries_command(args: &[&str]) -> i32 {
    let Some((verb, rest)) = args.split_first() else {
        eprintln!("kmp-mcp: summaries takes `pending [<about>] [--json]`");
        return 2;
    };
    if *verb != "pending" {
        eprintln!("kmp-mcp: summaries takes `pending [<about>] [--json]`, not `{verb}`");
        return 2;
    }
    let mut about = None;
    let mut json = false;
    for argument in rest {
        match *argument {
            "--json" => json = true,
            other if looks_like_option(other) => return unknown_option("summaries", other),
            other if about.is_none() => about = Some(other.to_string()),
            other => {
                eprintln!(
                    "kmp-mcp: summaries pending takes one about, and `{other}` is a second one"
                );
                return 2;
            }
        }
    }

    let resolved = match kmp_embedded::resolve_data_dir_from_env() {
        Ok(resolved) => resolved,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let bundle = kmp_embedded::EmbeddedKernelStore::open(resolved.path()).and_then(|store| {
        store.export_bundle_excluding_abouts_blocking(&ShippedGuideAbouts::owned())
    });
    let bundle = match bundle {
        Ok(bundle) => bundle,
        Err(error) => {
            eprintln!("kmp-mcp: {error}");
            return 2;
        }
    };
    let pending = match pending(&bundle, about.as_deref()) {
        Ok(pending) => pending,
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
            about
                .as_deref()
                .map(|about| format!(" in `{about}`"))
                .unwrap_or_default()
        );
        return 0;
    }
    println!(
        "{} {} an English search summary. Render each in plain English, keep every number, \
         identifier and acronym exactly as written, and attach it with kmp_write_memory \
         (intent record_summary, current.ref, current.summary_en).\n",
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
