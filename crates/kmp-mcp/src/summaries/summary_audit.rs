use super::audit_scope::AuditScope;
use super::audited_summary::AuditedSummary;
use super::bundle_entries::{about_lexicons, entry_histories};
use super::summary_judgement::judge;
use super::summary_totals::SummaryTotals;

use std::collections::HashMap;

/// One reading of the store's English search summaries.
///
/// This is the only reading there is. The doctor's count, the terminal's
/// list of what is owed and the agent's audit are three projections of it,
/// so they cannot disagree: a number that comes out of a second
/// implementation agrees with the first only until one of them is edited.
///
/// The reading itself is three steps in three files — decode the bundle into
/// each entry's history, build each about's lexical field, judge every entry
/// against both — and this type is what holds them in order.
#[derive(Debug, Clone)]
pub struct SummaryAudit {
    entries: Vec<AuditedSummary>,
}

impl SummaryAudit {
    /// Reads an exported bundle, in the order the store met its memories.
    ///
    /// The latest write of every entry decides what it is, the way
    /// `document` reads an about. Earlier writes are not discarded: they are
    /// what says whether the text moved after the summary did.
    pub fn read(bundle: &str, scope: &AuditScope) -> Result<Self, String> {
        let histories = entry_histories(bundle, scope)?;
        let lexicons = about_lexicons(&histories);
        Ok(Self {
            entries: histories
                .iter()
                .map(|history| {
                    judge(
                        history,
                        lexicons.get(&history.about).expect("one per about"),
                    )
                })
                .collect(),
        })
    }

    /// Every memory this reading covered, in the order the store met them.
    pub fn entries(&self) -> &[AuditedSummary] {
        &self.entries
    }

    /// The totals over the whole reading.
    pub fn totals(&self) -> SummaryTotals {
        SummaryTotals::of(&self.entries)
    }

    /// The totals per about, in the order the store met them.
    pub fn totals_by_about(&self) -> Vec<(String, SummaryTotals)> {
        let mut order: Vec<&str> = Vec::new();
        let mut grouped: HashMap<&str, Vec<&AuditedSummary>> = HashMap::new();
        for entry in &self.entries {
            if !grouped.contains_key(entry.about.as_str()) {
                order.push(&entry.about);
            }
            grouped.entry(&entry.about).or_default().push(entry);
        }
        order
            .into_iter()
            .map(|about| {
                (
                    about.to_string(),
                    SummaryTotals::of(grouped.remove(about).unwrap_or_default()),
                )
            })
            .collect()
    }

    /// The memories that owe a summary: what `kmp-mcp summaries pending`
    /// lists and the doctor counts.
    pub fn owed(&self) -> impl Iterator<Item = &AuditedSummary> {
        self.entries.iter().filter(|entry| entry.owes_a_summary())
    }
}

#[cfg(test)]
mod tests {
    use super::super::summary_state::SummaryState;
    use super::*;

    fn event(about: &str, id: &str, payload: &str) -> String {
        serde_json::json!({
            "root_node_id": about,
            "changes": [{
                "entity_kind": "memory_entry",
                "entity_id": id,
                "payload_json": payload
            }]
        })
        .to_string()
    }

    fn payload(text: &str, summary: Option<&str>) -> String {
        let mut value = serde_json::json!({"id": "x", "kind": "decision", "text": text});
        if let Some(summary) = summary {
            value["metadata"] = serde_json::json!({"summary_en": summary});
        }
        value.to_string()
    }

    fn audit(bundle: &str) -> SummaryAudit {
        SummaryAudit::read(bundle, &AuditScope::AllAbouts).expect("bundle parses")
    }

    #[test]
    fn the_reading_covers_every_memory_and_counts_only_the_debt_as_owed() {
        let bundle = [
            event(
                "project:a",
                "project:a:e1",
                &payload(
                    "El despliegue se retrasó porque los auditores no firmaron.",
                    None,
                ),
            ),
            event(
                "project:a",
                "project:a:e2",
                &payload(
                    "The rollout slipped because the auditors had not signed off.",
                    None,
                ),
            ),
            event(
                "project:a",
                "project:a:e3",
                &payload(
                    "Los auditores pidieron el registro completo antes del jueves.",
                    Some("the record"),
                ),
            ),
        ]
        .join("\n");

        let audit = audit(&bundle);
        let totals = audit.totals();

        assert_eq!(totals.entries, 3);
        assert_eq!(totals.missing, 1);
        assert_eq!(totals.refused, 1);
        assert_eq!(totals.not_required, 1);
        assert_eq!(totals.owed(), 2);
        assert_eq!(audit.owed().count(), 2);
        assert_eq!(
            audit
                .entries()
                .iter()
                .map(|entry| entry.state)
                .collect::<Vec<_>>(),
            [
                SummaryState::Missing,
                SummaryState::NotRequired,
                SummaryState::Refused
            ]
        );
    }

    #[test]
    fn a_named_set_narrows_the_reading_to_what_it_names() {
        let bundle = [
            event(
                "project:a",
                "project:a:e1",
                &payload("La válvula se congeló durante el turno de noche.", None),
            ),
            event(
                "project:b",
                "project:b:e1",
                &payload("El menú del comedor se publicó en el tablón.", None),
            ),
        ]
        .join("\n");

        let set = SummaryAudit::read(&bundle, &AuditScope::Abouts(vec!["project:b".to_string()]))
            .expect("parses");

        assert_eq!(set.totals().entries, 1);
        assert_eq!(set.owed().next().expect("one debt").about, "project:b");
    }

    #[test]
    fn the_totals_split_by_about_in_the_order_the_store_met_them() {
        let bundle = [
            event(
                "project:b",
                "project:b:e1",
                &payload("El menú se publicó en el tablón.", None),
            ),
            event(
                "project:a",
                "project:a:e1",
                &payload("The rollout slipped.", None),
            ),
        ]
        .join("\n");

        let totals = audit(&bundle).totals_by_about();

        assert_eq!(totals.len(), 2);
        assert_eq!(totals[0].0, "project:b");
        assert_eq!(totals[0].1.missing, 1);
        assert_eq!(totals[1].0, "project:a");
        assert_eq!(totals[1].1.not_required, 1);
    }

    /// The audit is a reading, not a judgement with a memory of its own: two
    /// readings of one bundle are the same reading.
    #[test]
    fn the_same_bundle_reads_the_same_way_twice() {
        let bundle = [
            event(
                "project:a",
                "project:a:e1",
                &payload("El despliegue se retrasó.", None),
            ),
            event(
                "project:a",
                "project:a:e2",
                &payload(
                    "La válvula se congeló.",
                    Some("The valve froze during the night shift."),
                ),
            ),
        ]
        .join("\n");

        assert_eq!(audit(&bundle).entries(), audit(&bundle).entries());
    }

    #[test]
    fn a_bundle_that_is_not_readable_is_named_rather_than_half_read() {
        let error =
            SummaryAudit::read("{\"root_node_id\":\"a\"}\nnot json", &AuditScope::AllAbouts)
                .expect_err("bad line");

        assert!(error.starts_with("bundle line 2 is not JSON"), "{error}");
    }
}
