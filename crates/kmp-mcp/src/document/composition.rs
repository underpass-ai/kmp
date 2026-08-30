//! How the document is put together: sections in kind order, entries
//! with their evidence, the lifecycle of supersessions, the time span.

use std::collections::HashMap;
use std::fmt::Write as _;

use serde_json::Value;

use super::entry::Entry;
use super::evidence::Evidence;
use super::markdown::*;
use super::relation::Relation;

pub(super) const KIND_ORDER: &[(&str, &str)] = &[
    ("decision", "Decisions"),
    ("constraint", "Constraints"),
    ("observation", "Observations"),
    ("success_path", "What worked"),
    ("error_path", "What did not"),
    ("feedback", "Feedback"),
    ("preference", "Preferences"),
    ("semantic_delta", "What changed"),
    ("derived_value", "Derived values"),
];

pub(super) fn text_of(value: &Value) -> Option<String> {
    let text = value.as_str()?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

pub(super) fn group_relations(relations: &[Relation]) -> HashMap<&str, Vec<&Relation>> {
    let mut grouped: HashMap<&str, Vec<&Relation>> = HashMap::new();
    for relation in relations {
        grouped
            .entry(relation.from.as_str())
            .or_default()
            .push(relation);
    }
    grouped
}

/// The heading for an entry. Entries carry text, not titles, so the first
/// line of the text is the heading — truncated the way the kernel truncates
/// its own node titles, so the document and the graph name things alike.
pub(super) fn write_document(
    about: &str,
    entries: &[Entry],
    outgoing: &HashMap<&str, Vec<&Relation>>,
    evidence: &HashMap<String, Vec<Evidence>>,
    relations: &[Relation],
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# {}\n", markdown_literal(about));

    let span = time_span(entries);
    let _ = writeln!(
        out,
        "{} {}, {} {}, {} {}{}.",
        entries.len(),
        plural(entries.len(), "entry", "entries"),
        relations.len(),
        plural(relations.len(), "relation", "relations"),
        evidence.values().map(Vec::len).sum::<usize>(),
        plural(
            evidence.values().map(Vec::len).sum::<usize>(),
            "evidence item",
            "evidence items"
        ),
        span.map(|span| format!(", {}", markdown_literal(&span)))
            .unwrap_or_default()
    );
    let _ = writeln!(
        out,
        "\nRendered from the event log by `kmp-mcp document`. Stored values are quoted as \
         literals and display controls are made visible; no model generated or rewrote their \
         wording.\n"
    );

    let mut rendered = vec![false; entries.len()];
    for (kind, title) in KIND_ORDER {
        write_section(
            &mut out,
            title,
            entries,
            &mut rendered,
            |entry| entry.kind == *kind,
            outgoing,
            evidence,
        );
    }
    // A kind the renderer has never heard of is still someone's memory. It
    // goes last rather than nowhere.
    write_section(
        &mut out,
        "Everything else",
        entries,
        &mut rendered,
        |_| true,
        outgoing,
        evidence,
    );
    debug_assert!(rendered.iter().all(|was_rendered| *was_rendered));

    write_lifecycle(&mut out, relations);
    out
}

#[allow(clippy::too_many_arguments)]
pub(super) fn write_section(
    out: &mut String,
    title: &str,
    entries: &[Entry],
    rendered: &mut [bool],
    include: impl Fn(&Entry) -> bool,
    outgoing: &HashMap<&str, Vec<&Relation>>,
    evidence: &HashMap<String, Vec<Evidence>>,
) {
    let selected: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(at, entry)| !rendered[*at] && include(entry))
        .map(|(at, _)| at)
        .collect();
    if selected.is_empty() {
        return;
    }
    let _ = writeln!(out, "## {title}\n");
    for at in selected {
        rendered[at] = true;
        write_entry(out, &entries[at], outgoing, evidence);
    }
}

pub(super) fn write_entry(
    out: &mut String,
    entry: &Entry,
    outgoing: &HashMap<&str, Vec<&Relation>>,
    evidence: &HashMap<String, Vec<Evidence>>,
) {
    let _ = writeln!(out, "### {}\n", markdown_literal(&heading(&entry.text)));
    let when = entry
        .observed_at
        .as_deref()
        .map(|when| format!(" · **Observed.** {}", markdown_literal(when)))
        .unwrap_or_default();
    // The ref stays visible: a reader who wants the stored object rather than
    // the prose can take it straight to `kmp_inspect`.
    let _ = writeln!(
        out,
        "**Ref.** {}{}\n",
        markdown_literal(&entry.reference),
        when
    );
    write_quoted_text(out, "", "Stored memory", &entry.text);
    let _ = writeln!(out);

    if let Some(items) = evidence.get(&entry.reference) {
        for item in items {
            write_quoted_text(out, "", "Evidence", &item.text);
            if let Some(source) = item.source.as_deref() {
                let _ = writeln!(out, "> **Source.** {}", markdown_literal(source));
            }
            let _ = writeln!(out);
        }
    }

    if let Some(links) = outgoing.get(entry.reference.as_str()) {
        for link in links {
            let _ = writeln!(
                out,
                "- **{}** {}",
                markdown_literal(&link.rel.replace('_', " ")),
                markdown_literal(&link.to)
            );
            if let Some(why) = &link.why {
                write_quoted_text(out, "  ", "Why", why);
            }
            if let Some(proof) = &link.evidence {
                write_quoted_text(out, "  ", "Relation evidence", proof);
            }
        }
        let _ = writeln!(out);
    }
}

/// Supersession and contradiction are different things and the document says
/// so. One is a lifecycle — the older entry is history, not advice. The other
/// is a live disagreement, and the tension is the information.
pub(super) fn write_lifecycle(out: &mut String, relations: &[Relation]) {
    for (rel, title, lead) in [
        (
            "supersedes",
            "What stopped being true",
            "Both are still in the log. The older one is what was true then, not what to do now.",
        ),
        (
            "contradicts",
            "What still disagrees",
            "Nothing was replaced here. Both entries are live, and the disagreement is the point.",
        ),
    ] {
        let matching: Vec<&Relation> = relations
            .iter()
            .filter(|relation| relation.rel == rel)
            .collect();
        if matching.is_empty() {
            continue;
        }
        let _ = writeln!(out, "## {title}\n\n{lead}\n");
        for relation in matching {
            let _ = writeln!(
                out,
                "- {} → {}",
                markdown_literal(&relation.to),
                markdown_literal(&relation.from)
            );
            if let Some(why) = &relation.why {
                write_quoted_text(out, "  ", "Why", why);
            }
        }
        let _ = writeln!(out);
    }
}

pub(super) fn time_span(entries: &[Entry]) -> Option<String> {
    let mut times: Vec<&str> = entries
        .iter()
        .filter_map(|entry| entry.observed_at.as_deref())
        .collect();
    times.sort_unstable();
    let first = times.first()?;
    let last = times.last()?;
    Some(if first == last {
        (*first).to_string()
    } else {
        format!("{first} to {last}")
    })
}

pub(super) fn plural<'a>(count: usize, one: &'a str, many: &'a str) -> &'a str {
    if count == 1 { one } else { many }
}
