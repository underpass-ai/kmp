//! One concept: one reading of the store's search summaries as the JSON an
//! agent reads.
//!
//! Mapping only. The totals and the per-about counts are the stable core —
//! they describe the whole selection and never move with the page — and each
//! memory becomes one item. What fits on a page, and how a caller asks for
//! the next one, belongs to `summaries_audit_page` and never here, the way
//! `inspect_projection` and `inspect_budget` divide the same work.

use serde_json::{Map, Value, json};

use crate::summaries::{AuditScope, AuditedSummary, SummaryAudit, SummaryTotals};

/// The part of the answer that describes the whole selection, with an empty
/// page the caller fills in.
pub(super) fn core_response(audit: &SummaryAudit, scope: &AuditScope) -> Value {
    let totals = audit.totals();
    json!({
        "summary": summary_sentence(&totals, scope),
        "scope": {
            "selection": scope.name(),
            "abouts": scope.named_abouts()
        },
        "totals": totals_json(&totals),
        "abouts": audit
            .totals_by_about()
            .into_iter()
            .map(|(about, totals)| json!({"about": about, "totals": totals_json(&totals)}))
            .collect::<Vec<_>>(),
        "entries": [],
        "page": Value::Null,
        "next_actions": [],
        "warnings": []
    })
}

/// One memory, and where it stands. A field the memory does not have is
/// absent rather than null: an audit of a large store pays for every key it
/// prints.
pub(super) fn audited_summary_json(entry: &AuditedSummary) -> Value {
    let mut item = Map::new();
    item.insert("about".to_string(), json!(entry.about));
    item.insert("ref".to_string(), json!(entry.reference));
    item.insert("kind".to_string(), json!(entry.kind));
    item.insert("state".to_string(), json!(entry.state.as_str()));
    if let Some(text) = &entry.text {
        item.insert("text".to_string(), json!(text));
    }
    if let Some(summary) = &entry.summary {
        item.insert("summary_en".to_string(), json!(summary));
    }
    if let Some(writer) = &entry.summary_by {
        item.insert("summary_en_by".to_string(), json!(writer));
    }
    if !entry.faults.is_empty() {
        item.insert("faults".to_string(), json!(entry.fault_sentences()));
    }
    if !entry.weaknesses.is_empty() {
        item.insert(
            "weaknesses".to_string(),
            json!(
                entry
                    .weaknesses
                    .iter()
                    .map(|weakness| json!({
                        "signal": weakness.name(),
                        "says": weakness.to_string()
                    }))
                    .collect::<Vec<_>>()
            ),
        );
    }
    Value::Object(item)
}

fn summary_sentence(totals: &SummaryTotals, scope: &AuditScope) -> String {
    let where_read = match scope {
        AuditScope::CurrentAbout(about) => format!("`{about}`"),
        AuditScope::Abouts(abouts) => format!("{} named abouts", abouts.len()),
        AuditScope::AllAbouts => "every about".to_string(),
    };
    if totals.entries == 0 {
        return format!("{where_read} holds no memory to audit");
    }
    format!(
        "{} of {} {} in {where_read} owe an English search summary; {} of the rest carr{} one \
         that stands and still retrieves little",
        totals.owed(),
        totals.entries,
        if totals.entries == 1 {
            "memory"
        } else {
            "memories"
        },
        totals.weak,
        if totals.weak == 1 { "ies" } else { "y" }
    )
}

fn totals_json(totals: &SummaryTotals) -> Value {
    json!({
        "entries": totals.entries,
        "missing": totals.missing,
        "refused": totals.refused,
        "stands": totals.stands,
        "not_required": totals.not_required,
        "weak": totals.weak,
        "owed": totals.owed()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(about: &str, id: &str, text: &str, summary: Option<&str>) -> String {
        let mut payload = json!({"id": id, "kind": "decision", "text": text});
        if let Some(summary) = summary {
            payload["metadata"] = json!({"summary_en": summary, "summary_en_by": "agent:a"});
        }
        json!({
            "root_node_id": about,
            "changes": [{
                "entity_kind": "memory_entry",
                "entity_id": id,
                "payload_json": payload.to_string()
            }]
        })
        .to_string()
    }

    fn audit(bundle: &str) -> SummaryAudit {
        SummaryAudit::read(bundle, &AuditScope::CurrentAbout("project:a".to_string()))
            .expect("bundle parses")
    }

    #[test]
    fn the_core_describes_the_whole_selection_and_leaves_the_page_to_its_caller() {
        let bundle = event(
            "project:a",
            "project:a:e1",
            "El despliegue se retrasó.",
            None,
        );

        let core = core_response(
            &audit(&bundle),
            &AuditScope::CurrentAbout("project:a".to_string()),
        );

        assert_eq!(core["totals"]["entries"], 1);
        assert_eq!(core["totals"]["owed"], 1);
        assert_eq!(core["abouts"][0]["about"], "project:a");
        assert_eq!(core["scope"]["selection"], "current_about");
        assert_eq!(core["entries"], json!([]));
        assert!(core["page"].is_null(), "the page is the pager's to fill");
    }

    #[test]
    fn an_empty_selection_says_so_rather_than_reporting_a_clean_store() {
        let core = core_response(
            &audit(""),
            &AuditScope::CurrentAbout("project:a".to_string()),
        );

        assert_eq!(core["summary"], "`project:a` holds no memory to audit");
    }

    #[test]
    fn a_sweep_and_a_named_set_each_say_what_they_read() {
        let bundle = event(
            "project:a",
            "project:a:e1",
            "El despliegue se retrasó.",
            None,
        );
        let audit = audit(&bundle);

        assert!(
            core_response(&audit, &AuditScope::AllAbouts)["summary"]
                .as_str()
                .expect("a sentence")
                .contains("in every about")
        );
        assert!(
            core_response(&audit, &AuditScope::Abouts(vec!["project:a".to_string()]))["summary"]
                .as_str()
                .expect("a sentence")
                .contains("in 1 named abouts")
        );
    }

    #[test]
    fn a_memory_with_nothing_to_do_prints_no_field_it_does_not_have() {
        let bundle = event(
            "project:a",
            "project:a:e1",
            "The rollout slipped because the auditors had not signed off.",
            None,
        );

        let item = audited_summary_json(&audit(&bundle).entries()[0]);

        assert_eq!(
            item.as_object()
                .expect("an object")
                .keys()
                .map(String::as_str)
                .collect::<std::collections::BTreeSet<_>>(),
            ["about", "kind", "ref", "state"].into()
        );
        assert_eq!(item["state"], "not_required");
    }

    #[test]
    fn a_refused_summary_prints_the_lints_words_and_who_wrote_it() {
        let bundle = event(
            "project:a",
            "project:a:e1",
            "Los auditores pidieron el registro completo antes del jueves.",
            Some("the record"),
        );

        let item = audited_summary_json(&audit(&bundle).entries()[0]);

        assert_eq!(item["state"], "refused");
        assert_eq!(item["summary_en"], "the record");
        assert_eq!(item["summary_en_by"], "agent:a");
        assert_eq!(
            item["faults"],
            json!(["carries 1 informative word, at least 2 are needed"])
        );
        assert!(item.get("weaknesses").is_none());
    }

    #[test]
    fn a_weakness_prints_a_word_to_branch_on_and_a_sentence_to_act_on() {
        let bundle = [
            event(
                "project:a",
                "project:a:e1",
                "La válvula de reserva se congeló de noche.",
                Some("The reserve valve froze during the night shift."),
            ),
            event(
                "project:a",
                "project:a:e1",
                "La bomba principal se detuvo de noche.",
                Some("The reserve valve froze during the night shift."),
            ),
        ]
        .join("\n");

        let item = audited_summary_json(&audit(&bundle).entries()[0]);

        assert_eq!(item["state"], "stands");
        assert_eq!(item["weaknesses"][0]["signal"], "stale");
        assert!(
            item["weaknesses"][0]["says"]
                .as_str()
                .expect("a sentence")
                .contains("render the text as it now stands")
        );
    }
}
