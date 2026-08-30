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

use serde_json::Value;

/// The kinds that get their own section, in the order a reader wants them:
/// what was decided, what bounds it, what was seen, what worked, what did
/// not, and what someone said about it.
mod composition;
mod entry;
mod evidence;
mod markdown;
mod ordered;
mod relation;

use composition::*;
use entry::Entry;
use evidence::Evidence;
use ordered::Ordered;
use relation::Relation;

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

#[cfg(test)]
mod tests {
    use super::render;

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
}
