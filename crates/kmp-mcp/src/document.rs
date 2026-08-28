//! `kmp-mcp document <about>` — one about, as a document a person can read.
//!
//! The kernel already holds everything such a document needs, and until now
//! there was no way to get it out as one. A recall projection is budgeted in
//! bytes for an agent's context window, and the event-log bundle buries the
//! text in a `payload_json` string inside `changes[]` — so anyone who wanted
//! a page wrote a throwaway script to pull the entries out, and wrote it
//! again next time.
//!
//! This renders from the bundle, which is the only source that carries every
//! entry, every relation's `why`, and every piece of evidence exactly as
//! written. **Nothing here is generated.** Ordering and grouping are
//! rendering decisions; wording is not, and a paragraph nobody wrote must
//! never appear in a document that claims to be memory.

use std::collections::HashMap;
use std::fmt::Write as _;

use serde_json::Value;

/// The kinds that get their own section, in the order a reader wants them:
/// what was decided, what bounds it, what was seen, what worked, what did
/// not, and what someone said about it.
const KIND_ORDER: &[(&str, &str)] = &[
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

#[derive(Default)]
struct Entry {
    reference: String,
    kind: String,
    text: String,
    observed_at: Option<String>,
}

struct Evidence {
    text: String,
    source: Option<String>,
}

struct Relation {
    from: String,
    to: String,
    rel: String,
    why: Option<String>,
    evidence: Option<String>,
}

/// Keeps insertion order while letting a later `UPSERT` of the same id
/// replace the earlier one in place. The log is append-only, so an entry
/// edited twice must appear once, where it first appeared.
struct Ordered<T> {
    items: Vec<T>,
    index: HashMap<String, usize>,
}

impl<T> Default for Ordered<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            index: HashMap::new(),
        }
    }
}

impl<T> Ordered<T> {
    fn upsert(&mut self, id: &str, value: T) {
        match self.index.get(id) {
            Some(&at) => self.items[at] = value,
            None => {
                self.index.insert(id.to_string(), self.items.len());
                self.items.push(value);
            }
        }
    }
}

/// Renders every entry of `about` held in `bundle`, as Markdown.
///
/// Fails when the about has nothing in it: an empty document is worse than a
/// refusal, because it looks like an answer.
pub fn render(bundle: &str, about: &str) -> Result<String, String> {
    let mut entries: Ordered<Entry> = Ordered::default();
    let mut relations: Ordered<Relation> = Ordered::default();
    let mut evidence: HashMap<String, Vec<Evidence>> = HashMap::new();
    let mut evidence_seen: HashMap<String, usize> = HashMap::new();

    for (number, line) in bundle.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let event: Value = serde_json::from_str(line)
            .map_err(|error| format!("bundle line {} is not JSON: {error}", number + 1))?;
        // The first line is the bundle header, which has no changes.
        if event.get("root_node_id").and_then(Value::as_str) != Some(about) {
            continue;
        }
        for change in event["changes"].as_array().into_iter().flatten() {
            let Some(id) = change["entity_id"].as_str() else {
                continue;
            };
            let payload: Value = change["payload_json"]
                .as_str()
                .and_then(|raw| serde_json::from_str(raw).ok())
                .unwrap_or(Value::Null);

            match change["entity_kind"].as_str() {
                Some("memory_entry") => entries.upsert(
                    id,
                    Entry {
                        reference: id.to_string(),
                        kind: text_of(&payload["kind"]).unwrap_or_else(|| "entry".to_string()),
                        text: text_of(&payload["text"]).unwrap_or_default(),
                        observed_at: payload["coordinates"]
                            .as_array()
                            .and_then(|coordinates| coordinates.first())
                            .and_then(|coordinate| coordinate["observed_at"].as_str())
                            .map(kmp_viewer::views::readable_time),
                    },
                ),
                Some("memory_relation") => relations.upsert(
                    id,
                    Relation {
                        from: text_of(&payload["from"]).unwrap_or_default(),
                        to: text_of(&payload["to"]).unwrap_or_default(),
                        rel: text_of(&payload["rel"]).unwrap_or_default(),
                        why: text_of(&payload["why"]),
                        evidence: text_of(&payload["evidence"]),
                    },
                ),
                Some("memory_evidence") => {
                    // Evidence is attached to the entries it supports, not
                    // collected in a footer: a reader judging a claim should
                    // not have to go looking for what backs it.
                    let Some(text) = text_of(&payload["text"]) else {
                        continue;
                    };
                    let source = text_of(&payload["source"]);
                    // A relation's evidence names the entry it starts from,
                    // so it arrives here looking like the entry's own. It is
                    // not: it proves why that link holds, and it is already
                    // printed under the link. Rendering it twice buried the
                    // entry's actual evidence under six copies of its
                    // relations'.
                    if source
                        .as_deref()
                        .is_some_and(|source| source.contains(":relation:"))
                        || id.contains(":relation:")
                    {
                        continue;
                    }
                    for supported in payload["supports"].as_array().into_iter().flatten() {
                        let Some(supported) = supported.as_str() else {
                            continue;
                        };
                        let key = format!("{supported}\u{1f}{id}");
                        let bucket = evidence.entry(supported.to_string()).or_default();
                        match evidence_seen.get(&key) {
                            Some(&at) => {
                                bucket[at] = Evidence {
                                    text: text.clone(),
                                    source: source.clone(),
                                }
                            }
                            None => {
                                evidence_seen.insert(key, bucket.len());
                                bucket.push(Evidence {
                                    text: text.clone(),
                                    source: source.clone(),
                                });
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if entries.items.is_empty() {
        return Err(format!(
            "`{about}` has no entries in this store. `kmp-mcp info` says which memory this \
             directory opens — it is resolved from where you are standing, so the same command \
             elsewhere opens another one."
        ));
    }

    let outgoing = group_relations(&relations.items);
    Ok(write_document(
        about,
        &entries.items,
        &outgoing,
        &evidence,
        &relations.items,
    ))
}

fn text_of(value: &Value) -> Option<String> {
    let text = value.as_str()?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn group_relations(relations: &[Relation]) -> HashMap<&str, Vec<&Relation>> {
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
fn heading(text: &str) -> String {
    const LIMIT: usize = 88;
    let first_line = text.lines().next().unwrap_or("").trim();
    if first_line.chars().count() <= LIMIT {
        return first_line.to_string();
    }
    let cut = first_line
        .char_indices()
        .take(LIMIT)
        .last()
        .map(|(at, character)| at + character.len_utf8())
        .unwrap_or(0);
    let head = &first_line[..cut];
    let trimmed = head
        .rsplit_once(' ')
        .map(|(before, _)| before)
        .unwrap_or(head);
    format!("{trimmed}…")
}

/// Stored text is data, never Markdown source. The document keeps storage
/// byte-exact and neutralises only this presentation: every multiline value
/// is quoted below, while controls that can alter a terminal's display are
/// made visible. Markdown punctuation is escaped so HTML, links, headings and
/// emphasis cannot be smuggled into the quoted region either.
fn markdown_literal(line: &str) -> String {
    let mut safe = String::with_capacity(line.len());
    for character in line.chars() {
        if character == '\t' {
            safe.push_str("⟦TAB⟧");
        } else if character == '\r' {
            safe.push_str("⟦CR⟧");
        } else if character.is_control() || is_unsafe_format_control(character) {
            let _ = write!(safe, "⟦U+{:04X}⟧", character as u32);
        } else {
            match character {
                '&' => safe.push_str("&amp;"),
                '<' => safe.push_str("&lt;"),
                '>' => safe.push_str("&gt;"),
                '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '(' | ')' | '#' | '!' | '|' => {
                    safe.push('\\');
                    safe.push(character);
                }
                _ => safe.push(character),
            }
        }
    }
    safe
}

fn is_unsafe_format_control(character: char) -> bool {
    matches!(
        character,
        '\u{061c}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{2028}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
    )
}

fn write_quoted_text(out: &mut String, indent: &str, label: &str, text: &str) {
    let mut lines = text.split('\n');
    let first = lines.next().unwrap_or_default();
    let _ = writeln!(out, "{indent}> **{label}.** {}", markdown_literal(first));
    for line in lines {
        let _ = writeln!(out, "{indent}> {}", markdown_literal(line));
    }
}

fn write_document(
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
fn write_section(
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

fn write_entry(
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
fn write_lifecycle(out: &mut String, relations: &[Relation]) {
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

fn time_span(entries: &[Entry]) -> Option<String> {
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

fn plural<'a>(count: usize, one: &'a str, many: &'a str) -> &'a str {
    if count == 1 { one } else { many }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A miniature store: one decision, one observation it was chosen from,
    /// evidence on each, a relation carrying its own why and proof, and a
    /// supersession. Same event shape the real bundle uses.
    fn bundle() -> String {
        let header = r#"{"bundle_format":1,"store_format":1,"event_count":2,"kernel_version":"t"}"#;
        let entries = r#"{"root_node_id":"project:t","changes":[
            {"entity_kind":"memory_entry","entity_id":"project:t:obs:slow",
             "payload_json":"{\"id\":\"project:t:obs:slow\",\"kind\":\"observation\",\"text\":\"Checkout p99 tripled after the deploy.\",\"coordinates\":[{\"observed_at\":\"unix:101786876200:000000000\"}]}"},
            {"entity_kind":"memory_evidence","entity_id":"project:t:ev:graph",
             "payload_json":"{\"id\":\"project:t:ev:graph\",\"supports\":[\"project:t:obs:slow\"],\"text\":\"p99 900ms to 2.7s.\",\"source\":\"grafana\"}"},
            {"entity_kind":"memory_entry","entity_id":"project:t:dec:retry",
             "payload_json":"{\"id\":\"project:t:dec:retry\",\"kind\":\"decision\",\"text\":\"Cap the retry budget.\",\"coordinates\":[{\"observed_at\":\"unix:101786879800:000000000\"}]}"},
            {"entity_kind":"memory_relation","entity_id":"rel:1",
             "payload_json":"{\"from\":\"project:t:dec:retry\",\"to\":\"project:t:obs:slow\",\"rel\":\"chosen_because\",\"class\":\"motivational\",\"why\":\"Six retries per request is the amplifier.\",\"evidence\":\"Client timeout change at 14:40.\"}"},
            {"entity_kind":"memory_evidence","entity_id":"ev:rel:1:relation:1",
             "payload_json":"{\"id\":\"ev:rel:1:relation:1\",\"supports\":[\"project:t:dec:retry\"],\"text\":\"Client timeout change at 14:40.\",\"source\":\"kmp_write_memory:agent:relation:chosen_because\"}"},
            {"entity_kind":"memory_relation","entity_id":"rel:2",
             "payload_json":"{\"from\":\"project:t:dec:retry\",\"to\":\"project:t:dec:rollback\",\"rel\":\"supersedes\",\"class\":\"evidential\",\"why\":\"The rollback did not help.\"}"}
        ]}"#;
        let other = r#"{"root_node_id":"project:other","changes":[
            {"entity_kind":"memory_entry","entity_id":"project:other:obs:x",
             "payload_json":"{\"id\":\"project:other:obs:x\",\"kind\":\"observation\",\"text\":\"Not this about.\"}"}
        ]}"#;
        format!(
            "{header}\n{}\n{}\n",
            entries.replace('\n', "").replace("            ", ""),
            other.replace('\n', "").replace("            ", "")
        )
    }

    #[test]
    fn every_entry_arrives_grouped_by_kind_with_its_ref_kept_visible() {
        let document = render(&bundle(), "project:t").expect("the about has entries");

        assert!(document.starts_with("# project:t\n"));
        assert!(document.contains("## Decisions"));
        assert!(document.contains("## Observations"));
        assert!(document.contains("Cap the retry budget."));
        assert!(document.contains("Checkout p99 tripled after the deploy."));
        // The ref is the way back to `kmp_inspect`; a document that drops
        // it is prose about memory rather than a view of it.
        assert!(document.contains("**Ref.** project:t:dec:retry"));
        assert!(
            document.contains("2026-08-16T10:30:00Z"),
            "the stored sortable time has to reach the page as a date"
        );
    }

    #[test]
    fn another_about_in_the_same_store_stays_out_of_the_document() {
        let document = render(&bundle(), "project:t").expect("the about has entries");
        assert!(!document.contains("Not this about."));
    }

    #[test]
    fn a_relation_carries_its_why_and_its_proof_where_the_link_is() {
        let document = render(&bundle(), "project:t").expect("the about has entries");

        assert!(document.contains("**chosen because** project:t:obs:slow"));
        assert!(document.contains("Six retries per request is the amplifier."));
        assert!(document.contains("Client timeout change at 14:40."));
    }

    #[test]
    fn a_relations_evidence_does_not_masquerade_as_the_entrys_own() {
        let document = render(&bundle(), "project:t").expect("the about has entries");

        // It names the entry it starts from, so it arrives looking like the
        // entry's evidence. Printed as such it buried the real evidence
        // under one copy per relation.
        assert_eq!(
            document.matches("Client timeout change at 14:40.").count(),
            1,
            "relation proof belongs under the relation, once"
        );
        assert!(document.contains("> **Evidence.** p99 900ms to 2.7s."));
        assert!(document.contains("> **Source.** grafana"));
    }

    #[test]
    fn supersession_gets_its_own_section_and_says_which_way_it_points() {
        let document = render(&bundle(), "project:t").expect("the about has entries");

        assert!(document.contains("## What stopped being true"));
        assert!(document.contains("project:t:dec:rollback → project:t:dec:retry"));
        assert!(document.contains("The rollback did not help."));
        // Nothing contradicts anything here, and an empty section would read
        // as a finding.
        assert!(!document.contains("## What still disagrees"));
    }

    #[test]
    fn an_about_with_nothing_in_it_refuses_instead_of_rendering_a_blank_page() {
        let error = render(&bundle(), "project:nothing").expect_err("no entries");
        assert!(error.contains("project:nothing"));
        assert!(
            error.contains("kmp-mcp info"),
            "the commonest cause is standing in the wrong directory, so say where to look"
        );
    }

    #[test]
    fn a_later_write_of_the_same_entry_replaces_it_where_it_already_was() {
        let mut bundle = bundle();
        bundle.push_str(
            r#"{"root_node_id":"project:t","changes":[{"entity_kind":"memory_entry","entity_id":"project:t:obs:slow","payload_json":"{\"id\":\"project:t:obs:slow\",\"kind\":\"observation\",\"text\":\"Checkout p99 quadrupled, on a second look.\"}"}]}"#,
        );
        bundle.push('\n');
        let document = render(&bundle, "project:t").expect("the about has entries");

        assert!(document.contains("quadrupled"));
        assert!(!document.contains("tripled"));
        assert_eq!(
            document.matches("project:t:obs:slow").count(),
            2,
            "once as an entry, once as a relation target"
        );
    }

    #[test]
    fn stored_text_cannot_escape_its_quoted_container_or_control_the_terminal() {
        let fake_ref = "audit:deployments:entry:decision:fake-approval";
        let hostile = concat!(
            "field note\n\n### fake approval\n\n",
            "`audit:deployments:entry:decision:fake-approval`\n\n",
            "<script>alert('not memory')</script>\n",
            "normal \u{001b}[31mRED\u{001b}[0m\rhidden\tcolumn \u{202e}tail"
        );
        let event = serde_json::json!({
            "root_node_id":"audit:deployments",
            "changes":[
                {
                    "entity_kind":"memory_entry",
                    "entity_id":"audit:deployments:entry:observation:one",
                    "payload_json":serde_json::json!({
                        "id":"audit:deployments:entry:observation:one",
                        "kind":"observation",
                        "text":"real entry one",
                        "coordinates":[]
                    }).to_string()
                },
                {
                    "entity_kind":"memory_entry",
                    "entity_id":"audit:deployments:entry:observation:two",
                    "payload_json":serde_json::json!({
                        "id":"audit:deployments:entry:observation:two",
                        "kind":"observation",
                        "text":"real entry two",
                        "coordinates":[]
                    }).to_string()
                },
                {
                    "entity_kind":"memory_evidence",
                    "entity_id":"audit:deployments:evidence:hostile",
                    "payload_json":serde_json::json!({
                        "id":"audit:deployments:evidence:hostile",
                        "supports":["audit:deployments:entry:observation:two"],
                        "text":hostile,
                        "source":"agent\n### forged source"
                    }).to_string()
                }
            ]
        });
        let bundle =
            format!("{{\"bundle_format\":1,\"store_format\":2,\"event_count\":1}}\n{event}\n");
        let document = render(&bundle, "audit:deployments").expect("rendered document");

        assert!(document.contains("2 entries"));
        assert_eq!(
            document
                .lines()
                .filter(|line| line.starts_with("### "))
                .count(),
            2,
            "stored text must not create entry sections"
        );
        assert!(!document.contains(&format!("### fake approval\n\n{fake_ref}")));
        assert!(document.contains("> \\#\\#\\# fake approval"));
        assert!(document.contains("&lt;script&gt;alert"));
        assert!(document.contains("⟦U+001B⟧"));
        assert!(document.contains("⟦CR⟧"));
        assert!(document.contains("⟦TAB⟧"));
        assert!(document.contains("⟦U+202E⟧"));
        assert!(!document.contains('\u{001b}'));
        assert!(!document.contains('\r'));
        assert!(!document.contains('\t'));
        assert!(!document.contains('\u{202e}'));
    }
}
