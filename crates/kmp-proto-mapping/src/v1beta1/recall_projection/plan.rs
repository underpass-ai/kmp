//! The canonical projection plan: the stable core every page repeats and
//! the expansion items, in the one total order the cursor is bound to.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::budget::{Detail, ProjectionBudget};
use super::json_paths::{array_at_mut, array_len, push_array, take_array};
use super::normalization::{cited_evidence_refs, rebuild_answer, wake_evidence_refs};
use super::scalars::u64_at;

/// The head of the catalogue: the labels most entries stand in, the current
/// about first, up to this many and this many serialized bytes. A writer
/// reads them before naming a label, so they are the first expansion the
/// packet fills, ahead of the state lines beyond the first; the rest of the
/// catalogue follows the causal spine. Nothing enters the core: the prose
/// and the cited proof are never shortened to make room for a label.
const LABEL_HEAD_ITEMS: usize = 8;
const LABEL_HEAD_BYTES: usize = 800;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum Section {
    WakeCurrentState,
    WakeCausalSpine,
    WakeOpenLoops,
    WakeNextActions,
    WakeGuardrails,
    ProofEvidence,
    ProofPath,
    ProofMissing,
    Labels,
}

impl Section {
    pub(super) const ALL: [Self; 9] = [
        Self::WakeCurrentState,
        Self::WakeCausalSpine,
        Self::WakeOpenLoops,
        Self::WakeNextActions,
        Self::WakeGuardrails,
        Self::ProofEvidence,
        Self::ProofPath,
        Self::ProofMissing,
        Self::Labels,
    ];

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::WakeCurrentState => "wake.current_state",
            Self::WakeCausalSpine => "wake.causal_spine",
            Self::WakeOpenLoops => "wake.open_loops",
            Self::WakeNextActions => "wake.next_actions",
            Self::WakeGuardrails => "wake.guardrails",
            Self::ProofEvidence => "proof.evidence",
            Self::ProofPath => "proof.path",
            Self::ProofMissing => "proof.missing",
            Self::Labels => "labels",
        }
    }

    pub(super) fn path(self) -> &'static [&'static str] {
        match self {
            Self::WakeCurrentState => &["wake", "current_state"],
            Self::WakeCausalSpine => &["wake", "causal_spine"],
            Self::WakeOpenLoops => &["wake", "open_loops"],
            Self::WakeNextActions => &["wake", "next_actions"],
            Self::WakeGuardrails => &["wake", "guardrails"],
            Self::ProofEvidence => &["proof", "evidence"],
            Self::ProofPath => &["proof", "path"],
            Self::ProofMissing => &["proof", "missing"],
            Self::Labels => &["labels"],
        }
    }
}

#[derive(Clone)]
pub(super) struct ProjectionItem {
    pub(super) section: Section,
    pub(super) value: Value,
    pub(super) min_detail: Detail,
    pub(super) priority: u8,
    pub(super) stable_key: String,
}

pub(super) struct ProjectionPlan {
    pub(super) core: Value,
    pub(super) items: Vec<ProjectionItem>,
    pub(super) core_lengths: BTreeMap<Section, usize>,
    pub(super) selection_omitted: usize,
    pub(super) arguments: Value,
    pub(super) progress_bytes: usize,
}

impl ProjectionPlan {
    pub(super) fn build(mut value: Value, budget: &ProjectionBudget) -> Self {
        // Mapping may already have capped the ranked evidence. Those items
        // are no longer here to count and must not become detail exclusions.
        let mut selection_omitted =
            usize::try_from(u64_at(&value, "/projection/selection_omitted")).unwrap_or(usize::MAX);
        if let Some(max_entries) = budget.max_entries
            && let Some(reasons) = array_at_mut(&mut value, &["because"])
            && reasons.len() > max_entries
        {
            selection_omitted += reasons.len() - max_entries;
            reasons.truncate(max_entries);
            rebuild_answer(&mut value);
        }

        let mut items = Vec::new();
        if value.get("wake").is_some() {
            // State lines beyond the first sit at 1 so the catalogue head,
            // at 0, is the one expansion that precedes them.
            for (section, min_detail, priority) in [
                (Section::WakeCurrentState, Detail::Compact, 1),
                (Section::WakeCausalSpine, Detail::Compact, 5),
                (Section::WakeOpenLoops, Detail::Balanced, 10),
                (Section::WakeNextActions, Detail::Balanced, 11),
                (Section::WakeGuardrails, Detail::Balanced, 12),
            ] {
                let mut values = take_array(&mut value, section.path());
                if !values.is_empty() {
                    push_array(&mut value, section.path(), values.remove(0));
                }
                items.extend(
                    values
                        .into_iter()
                        .map(|value| ProjectionItem::new(section, value, min_detail, priority)),
                );
            }
            // The catalogue is expansion in its own order: a writer needs
            // its head before naming a label, and a filter without it is a
            // guess, so the head goes first; the tail follows the causal
            // spine, ahead of the proof path. The stable key carries the
            // catalogue's rank, so paging keeps labels by use rather than by
            // their JSON.
            let mut head_bytes = 0usize;
            for (index, label) in take_array(&mut value, &["labels"]).into_iter().enumerate() {
                let bytes = serde_json::to_string(&label)
                    .map(|text| text.len())
                    .unwrap_or(0);
                let in_head = index < LABEL_HEAD_ITEMS && head_bytes + bytes <= LABEL_HEAD_BYTES;
                if in_head {
                    head_bytes += bytes;
                }
                items.push(ProjectionItem {
                    section: Section::Labels,
                    value: label,
                    min_detail: Detail::Compact,
                    priority: if in_head { 0 } else { 6 },
                    stable_key: format!("labels:{index:05}"),
                });
            }
        }

        let required_refs = cited_evidence_refs(&value)
            .union(&wake_evidence_refs(&value))
            .cloned()
            .collect::<BTreeSet<_>>();
        let evidence_items = take_array(&mut value, &["proof", "evidence"]);
        let required_count = evidence_items
            .iter()
            .filter(|evidence| {
                evidence
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| required_refs.contains(id))
            })
            .count();
        let extra_limit = budget
            .max_entries
            .map(|limit| limit.saturating_sub(required_count))
            .unwrap_or(usize::MAX);
        let mut extras_retained = 0usize;
        let preserve_evidence_rank = value.get("because").is_some();
        for (rank, evidence) in evidence_items.into_iter().enumerate() {
            let evidence_id = evidence
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if required_refs.contains(evidence_id) {
                push_array(&mut value, &["proof", "evidence"], evidence);
            } else if extras_retained < extra_limit {
                let mut item =
                    ProjectionItem::new(Section::ProofEvidence, evidence, Detail::Balanced, 20);
                if preserve_evidence_rank {
                    // Ask already ranked these candidates. JSON identity is
                    // only a tie-breaker, not a second retrieval policy. Keep
                    // the serialized value in the key so the cursor remains
                    // bound to both this order and the exact evidence content.
                    item.stable_key = format!("proof.evidence:{rank:020}:{}", item.stable_key);
                }
                items.push(item);
                extras_retained += 1;
            } else {
                selection_omitted += 1;
            }
        }
        for relation in take_array(&mut value, &["proof", "path"]) {
            let (min_detail, priority) = relation_priority(&relation);
            items.push(ProjectionItem::new(
                Section::ProofPath,
                relation,
                min_detail,
                priority,
            ));
        }
        for missing in take_array(&mut value, &["proof", "missing"]) {
            items.push(ProjectionItem::new(
                Section::ProofMissing,
                missing,
                Detail::Full,
                90,
            ));
        }
        items.sort_by(|left, right| {
            (left.min_detail, left.priority, &left.stable_key).cmp(&(
                right.min_detail,
                right.priority,
                &right.stable_key,
            ))
        });
        let core_lengths = section_lengths(&value);
        Self {
            core: value,
            items,
            core_lengths,
            selection_omitted,
            arguments: Value::Null,
            progress_bytes: 0,
        }
    }
}

impl ProjectionItem {
    fn new(section: Section, value: Value, min_detail: Detail, priority: u8) -> Self {
        let stable_key = serde_json::to_string(&value).expect("projection item should serialize");
        Self {
            section,
            value,
            min_detail,
            priority,
            stable_key,
        }
    }
}

fn relation_priority(relation: &Value) -> (Detail, u8) {
    if relation.get("class").and_then(Value::as_str) == Some("structural") {
        return (Detail::Full, 80);
    }
    match relation
        .get("rel")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "supports" | "has_evidence" | "records" | "contains_entry" => (Detail::Balanced, 40),
        _ => (Detail::Compact, 10),
    }
}

pub(super) fn section_lengths(value: &Value) -> BTreeMap<Section, usize> {
    Section::ALL
        .into_iter()
        .map(|section| (section, array_len(value, section.path())))
        .collect()
}
