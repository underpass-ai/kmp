//! A relabel: the labels an entry stands in change, and its text does not.
//!
//! Translated the way an ingest is — against what the about holds — into
//! one `memory_relabel` change the log keeps and the projection reads as
//! `contains_entry` edges added and removed. A label added late inherits
//! the entry's clocks, because an entry's time does not depend on its
//! latest label; the relabel's own instant lives in the event and on the
//! edge it added, never on the entry.

use std::collections::{BTreeMap, BTreeSet};

use kmp_domain::{
    MemoryDimensionIdentity, SourceKind, TemporalCoordinate, compare_temporal_instants,
    label_resemblances,
};

use crate::ApplicationError;
use crate::commands::{UpdateContextChange, UpdateContextCommand};
use crate::memory::{
    EntryLabelData, ExistingMemoryRefs, LabelPolicy, MemoryCoordinateData, MemoryDimensionData,
    MemoryRelabelCommand, MemoryRelabelOutcome, ResemblingLabelData,
};

use super::ref_boundary::{validate_ref_token, validate_supplied_entry_ref};

/// The `method` a `contains_entry` edge carries when a relabel put it
/// there, so a reader can tell a label given at write from one given later.
pub const RELABEL_METHOD: &str = "kmp_relabel";

/// The change kind a relabel appends to the log. The projection reads it as
/// edges added and removed around one entry; nothing else in the log does.
pub const RELABEL_ENTITY_KIND: &str = "memory_relabel";

/// Translates a relabel into the one change the log keeps, refusing what
/// only the caller can fix: a label the entry already stands in, one it
/// does not stand in, a value already used under another key, the last
/// label an entry has, or a new label that resembles one the about holds
/// under a policy that refuses it.
pub fn translate_memory_relabel(
    command: &MemoryRelabelCommand,
    existing: &ExistingMemoryRefs,
    current: &[TemporalCoordinate],
) -> Result<(UpdateContextCommand, MemoryRelabelOutcome), ApplicationError> {
    validate_command(command)?;
    if !existing.refs.contains(&command.ref_id) {
        return Err(ApplicationError::NotFound(format!(
            "`{}` is not a memory of `{}`",
            command.ref_id, command.about
        )));
    }
    if current.is_empty() {
        return Err(ApplicationError::Validation(format!(
            "`{}` stands in no label, so it is not an entry that can be relabelled",
            command.ref_id
        )));
    }

    let standing = standing_labels(current);
    let removed = removals(command, &standing)?;
    let additions = additions(command, existing, &standing, &removed, current)?;

    let mut labels = standing.keys().cloned().collect::<BTreeSet<_>>();
    for (label, _) in &removed {
        labels.remove(label);
    }
    for added in &additions.added {
        labels.insert(added.clone());
    }
    if labels.is_empty() {
        return Err(ApplicationError::Validation(format!(
            "`{}` would stand in no label; an entry stands in at least one, which is where its time lives. Add a label before taking the last one off",
            command.ref_id
        )));
    }

    if command.label_policy == LabelPolicy::Refuse && !additions.resembling.is_empty() {
        return Err(ApplicationError::Validation(format!(
            "labels resemble ones the about already holds: {}. Reuse the existing label, or name the key in `intended_new` to insist on the new one",
            additions
                .resembling
                .iter()
                .map(|label| label.why.clone())
                .collect::<Vec<_>>()
                .join(" ")
        )));
    }

    let mut changes = Vec::new();
    for dimension in &additions.dimensions {
        changes.push(change(
            "memory_dimension",
            &dimension.id,
            serde_json::to_string(dimension),
            "KMP memory dimension ingest",
            vec![dimension.id.clone()],
        )?);
    }
    let provenance = command.provenance.as_ref();
    let payload = serde_json::json!({
        "ref": command.ref_id,
        "add": additions.coordinates,
        "remove": removed
            .iter()
            .map(|(label, scope_id)| serde_json::json!({
                "dimension": label.key,
                "scope_id": scope_id,
            }))
            .collect::<Vec<_>>(),
        "why": command.why.trim(),
        "actor": provenance.map(|provenance| provenance.source_agent.as_str()),
        "observed_at": provenance.map(|provenance| provenance.observed_at.as_str()),
    });
    let mut scopes = additions
        .coordinates
        .iter()
        .map(|coordinate| coordinate.scope_id.clone())
        .collect::<Vec<_>>();
    scopes.extend(removed.iter().map(|(_, scope_id)| scope_id.clone()));
    changes.push(change(
        RELABEL_ENTITY_KIND,
        &command.ref_id,
        serde_json::to_string(&payload),
        command.why.trim(),
        scopes,
    )?);

    let outcome = MemoryRelabelOutcome {
        about: command.about.clone(),
        ref_id: command.ref_id.clone(),
        added: additions.added,
        removed: removed.into_iter().map(|(label, _)| label).collect(),
        labels: labels.into_iter().collect(),
        created_dimensions: additions.created_dimensions,
        warnings: additions
            .resembling
            .iter()
            .map(|label| label.why.clone())
            .collect(),
        resembling_labels: additions.resembling,
        read_after_write_ready: false,
    };

    Ok((
        UpdateContextCommand {
            root_node_id: command.about.clone(),
            role: "memory".to_string(),
            work_item_id: command.idempotency_key.clone(),
            changes,
            expected_revision: None,
            expected_content_hash: None,
            idempotency_key: Some(command.idempotency_key.clone()),
            logical_digest: Some(logical_digest(command)),
            requested_by: provenance.map(|provenance| provenance.source_agent.clone()),
        },
        outcome,
    ))
}

/// What the additions of one relabel translate to: the labels as pairs, the
/// coordinates the entry gains, the dimensions declared for the first time,
/// and the resemblances the catalogue turned up.
struct Additions {
    added: Vec<EntryLabelData>,
    coordinates: Vec<MemoryCoordinateData>,
    dimensions: Vec<MemoryDimensionData>,
    created_dimensions: Vec<String>,
    resembling: Vec<ResemblingLabelData>,
}

fn validate_command(command: &MemoryRelabelCommand) -> Result<(), ApplicationError> {
    require_non_empty(&command.about, "about")?;
    validate_ref_token("about", &command.about).map_err(ApplicationError::Validation)?;
    require_non_empty(&command.ref_id, "ref")?;
    validate_supplied_entry_ref(&command.about, "ref", &command.ref_id)
        .map_err(ApplicationError::Validation)?;
    require_non_empty(&command.why, "why")?;
    require_non_empty(&command.idempotency_key, "idempotency_key")?;
    if command.add.is_empty() && command.remove.is_empty() {
        return Err(ApplicationError::Validation(
            "nothing to relabel: give `add`, `remove` or both".to_string(),
        ));
    }
    if let Some(provenance) = command.provenance.as_ref() {
        SourceKind::parse(&provenance.source_kind).map_err(|error| {
            ApplicationError::Validation(format!(
                "memory provenance source_kind is invalid: {error}"
            ))
        })?;
        require_non_empty(&provenance.source_agent, "provenance.source_agent")?;
        require_non_empty(&provenance.observed_at, "provenance.observed_at")?;
    }
    Ok(())
}

/// The labels the entry stands in now, each with the coordinate that says
/// so, keyed by the pair a caller names them by.
fn standing_labels(
    current: &[TemporalCoordinate],
) -> BTreeMap<EntryLabelData, &TemporalCoordinate> {
    current
        .iter()
        .map(|coordinate| {
            (
                EntryLabelData {
                    key: coordinate.dimension().to_string(),
                    value: bare_value(coordinate.scope_id()),
                },
                coordinate,
            )
        })
        .collect()
}

fn bare_value(scope_id: &str) -> String {
    MemoryDimensionIdentity::parse(scope_id)
        .map(|identity| identity.dimension_id().to_string())
        .unwrap_or_else(|| scope_id.trim().to_string())
}

/// The labels to take off, each with the scope id of the edge that goes.
/// One the entry does not stand in is refused naming what it does stand in.
fn removals(
    command: &MemoryRelabelCommand,
    standing: &BTreeMap<EntryLabelData, &TemporalCoordinate>,
) -> Result<Vec<(EntryLabelData, String)>, ApplicationError> {
    let mut seen = BTreeSet::new();
    let mut removed = Vec::new();
    for label in &command.remove {
        let label = normalized_label(label, "remove[]")?;
        if !seen.insert(label.clone()) {
            return Err(ApplicationError::Validation(format!(
                "`{}={}` is given twice in `remove`",
                label.key, label.value
            )));
        }
        let Some(coordinate) = standing.get(&label) else {
            return Err(ApplicationError::Validation(format!(
                "`{}` does not stand in `{}={}`; it stands in {}",
                command.ref_id,
                label.key,
                label.value,
                describe_labels(standing.keys())
            )));
        };
        removed.push((label, coordinate.scope_id().to_string()));
    }
    Ok(removed)
}

fn additions(
    command: &MemoryRelabelCommand,
    existing: &ExistingMemoryRefs,
    standing: &BTreeMap<EntryLabelData, &TemporalCoordinate>,
    removed: &[(EntryLabelData, String)],
    current: &[TemporalCoordinate],
) -> Result<Additions, ApplicationError> {
    let catalogue = existing
        .labels
        .iter()
        .map(|(kind, value)| (kind.as_str(), value.as_str()))
        .collect::<Vec<_>>();
    let clocks = inherited_clocks(current);
    let mut max_sequences = existing.max_sequences.clone();
    let mut added = Vec::new();
    let mut coordinates = Vec::new();
    let mut dimensions = Vec::new();
    let mut created_dimensions = Vec::new();
    let mut resembling = Vec::new();
    let mut values_added = BTreeMap::<String, String>::new();

    for label in &command.add {
        let label = normalized_label(label, "add[]")?;
        let identity = MemoryDimensionIdentity::resolve(&command.about, &label.value)
            .ok_or_else(|| {
                ApplicationError::Validation(format!(
                    "`add[].value` `{}` belongs to another about; name it bare or namespaced for `{}`",
                    label.value, command.about
                ))
            })?;
        let label = EntryLabelData {
            key: label.key,
            value: identity.dimension_id().to_string(),
        };
        if let Some(other_key) = values_added.get(&label.value) {
            return Err(ApplicationError::Validation(format!(
                "`add` uses `{}` under `{other_key}` and `{}`; within an about a scope id names one label and keeps the kind of its first use, so one id cannot be two kinds",
                label.value, label.key
            )));
        }
        if removed.iter().any(|(removed, _)| *removed == label) {
            return Err(ApplicationError::Validation(format!(
                "`{}={}` is both added and removed",
                label.key, label.value
            )));
        }
        if standing.contains_key(&label) {
            return Err(ApplicationError::Validation(format!(
                "`{}` already stands in `{}={}`; it stands in {}",
                command.ref_id,
                label.key,
                label.value,
                describe_labels(standing.keys())
            )));
        }
        values_added.insert(label.value.clone(), label.key.clone());

        let scope_id = identity.node_id();
        if existing.dimensions.contains(&scope_id) {
            // The about holds this scope already: its kind was fixed at
            // first use, and a relabel reuses it or names the clash.
            if let Some((kind, _)) = existing
                .labels
                .iter()
                .find(|(_, value)| *value == label.value)
                && *kind != label.key
            {
                return Err(ApplicationError::Validation(format!(
                    "`{}` already names the label `{kind}={}` in `{}`; within an about a scope id names one label and keeps the kind of its first use",
                    label.value, label.value, command.about
                )));
            }
        } else {
            if !command.intended_new.contains(&label.key) {
                resembling.extend(
                    label_resemblances(&label.key, &label.value, catalogue.iter().copied())
                        .into_iter()
                        .map(|resemblance| ResemblingLabelData {
                            key: resemblance.key().to_string(),
                            value: resemblance.value().to_string(),
                            existing_key: resemblance.existing_key().to_string(),
                            existing_value: resemblance.existing_value().to_string(),
                            kind: resemblance.kind().name().to_string(),
                            why: resemblance.why(),
                        }),
                );
            }
            let mut metadata = BTreeMap::new();
            metadata.insert("memory_about".to_string(), command.about.clone());
            metadata.insert("memory_dimension_id".to_string(), label.value.clone());
            dimensions.push(MemoryDimensionData {
                id: scope_id.clone(),
                kind: label.key.clone(),
                title: Some(format!("{}={}", label.key, label.value)),
                metadata,
            });
            created_dimensions.push(scope_id.clone());
        }

        let frontier = max_sequences
            .entry((label.key.clone(), scope_id.clone()))
            .or_default();
        *frontier = frontier.checked_add(1).ok_or_else(|| {
            ApplicationError::Validation(
                "memory coordinate sequence space is exhausted".to_string(),
            )
        })?;
        coordinates.push(MemoryCoordinateData {
            dimension: label.key.clone(),
            scope_id,
            occurred_at: clocks.occurred_at.clone(),
            observed_at: clocks.observed_at.clone(),
            ingested_at: clocks.ingested_at.clone(),
            valid_from: clocks.valid_from.clone(),
            valid_until: clocks.valid_until.clone(),
            sequence: Some(*frontier),
            rank: None,
            metadata: BTreeMap::new(),
        });
        added.push(label);
    }

    Ok(Additions {
        added,
        coordinates,
        dimensions,
        created_dimensions,
        resembling,
    })
}

/// The clocks a label added late inherits: the entry's earliest start on
/// each clock and its latest end, read off the coordinates it already has.
/// Every coordinate a writer emits shares the same clocks, so for those the
/// choice is moot; it matters only for an entry catalogued by hand.
struct InheritedClocks {
    occurred_at: Option<String>,
    observed_at: Option<String>,
    ingested_at: Option<String>,
    valid_from: Option<String>,
    valid_until: Option<String>,
}

fn inherited_clocks(current: &[TemporalCoordinate]) -> InheritedClocks {
    InheritedClocks {
        occurred_at: earliest(current.iter().filter_map(TemporalCoordinate::occurred_at)),
        observed_at: earliest(current.iter().filter_map(TemporalCoordinate::observed_at)),
        ingested_at: earliest(current.iter().filter_map(TemporalCoordinate::ingested_at)),
        valid_from: earliest(current.iter().filter_map(TemporalCoordinate::valid_from)),
        valid_until: latest(current.iter().filter_map(TemporalCoordinate::valid_until)),
    }
}

fn earliest<'a>(instants: impl Iterator<Item = &'a str>) -> Option<String> {
    instants
        .reduce(
            |kept, candidate| match compare_temporal_instants(candidate, kept) {
                Some(std::cmp::Ordering::Less) => candidate,
                _ => kept,
            },
        )
        .map(str::to_string)
}

fn latest<'a>(instants: impl Iterator<Item = &'a str>) -> Option<String> {
    instants
        .reduce(
            |kept, candidate| match compare_temporal_instants(candidate, kept) {
                Some(std::cmp::Ordering::Greater) => candidate,
                _ => kept,
            },
        )
        .map(str::to_string)
}

fn normalized_label(
    label: &EntryLabelData,
    field: &str,
) -> Result<EntryLabelData, ApplicationError> {
    let key = label.key.trim();
    let value = label.value.trim();
    require_non_empty(key, &format!("{field}.key"))?;
    require_non_empty(value, &format!("{field}.value"))?;
    validate_ref_token(&format!("{field}.key"), key).map_err(ApplicationError::Validation)?;
    validate_ref_token(&format!("{field}.value"), value).map_err(ApplicationError::Validation)?;
    Ok(EntryLabelData {
        key: key.to_string(),
        value: value.to_string(),
    })
}

fn describe_labels<'a>(labels: impl Iterator<Item = &'a EntryLabelData>) -> String {
    let described = labels
        .map(|label| format!("`{}={}`", label.key, label.value))
        .collect::<Vec<_>>();
    if described.is_empty() {
        "no label".to_string()
    } else {
        described.join(", ")
    }
}

fn change(
    entity_kind: &str,
    entity_id: &str,
    payload: Result<String, serde_json::Error>,
    reason: &str,
    scopes: Vec<String>,
) -> Result<UpdateContextChange, ApplicationError> {
    Ok(UpdateContextChange {
        operation: "UPSERT".to_string(),
        entity_kind: entity_kind.to_string(),
        entity_id: entity_id.to_string(),
        payload_json: payload.map_err(|error| {
            ApplicationError::Validation(format!("relabel payload could not serialize: {error}"))
        })?,
        reason: reason.to_string(),
        scopes,
    })
}

fn require_non_empty(value: &str, field: &str) -> Result<(), ApplicationError> {
    if value.trim().is_empty() {
        Err(ApplicationError::Validation(format!(
            "{field} cannot be empty"
        )))
    } else {
        Ok(())
    }
}

/// Digest of the logical relabel, taken before translation: what the caller
/// said, which is what a replay under the same idempotency key must equal.
fn logical_digest(command: &MemoryRelabelCommand) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(command.about.as_bytes());
    hasher.update([0]);
    hasher.update(command.ref_id.as_bytes());
    hasher.update([0]);
    let labels = serde_json::to_vec(&(&command.add, &command.remove))
        .expect("labels serialize: they hold only strings");
    hasher.update(&labels);
    hasher.update([0]);
    hasher.update(command.why.trim().as_bytes());
    hasher.update([0]);
    if let Some(provenance) = &command.provenance {
        let provenance =
            serde_json::to_vec(provenance).expect("provenance serializes: it holds only strings");
        hasher.update(&provenance);
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use kmp_domain::{RelationExplanation, RelationSemanticClass, TemporalCoordinate};

    use crate::ApplicationError;
    use crate::memory::{
        EntryLabelData, ExistingMemoryRefs, LabelPolicy, MemoryProvenanceData, MemoryRelabelCommand,
    };

    use super::{RELABEL_ENTITY_KIND, translate_memory_relabel};

    const ABOUT: &str = "project:kmp";
    const REF: &str = "project:kmp:decision:relabel";
    const PROCESS: &str = "about:project:kmp:dimension:harness";
    const TASK: &str = "about:project:kmp:dimension:launch";

    fn label(key: &str, value: &str) -> EntryLabelData {
        EntryLabelData {
            key: key.to_string(),
            value: value.to_string(),
        }
    }

    fn coordinate(
        kind: &str,
        scope_id: &str,
        occurred_at: &str,
        sequence: u32,
    ) -> TemporalCoordinate {
        TemporalCoordinate::from_relation_explanation(
            &RelationExplanation::new(RelationSemanticClass::Structural)
                .with_dimension(kind)
                .with_scope_id(scope_id)
                .with_occurred_at(occurred_at)
                .with_observed_at(occurred_at)
                .with_ingested_at("unix:101788000000:000000000")
                .with_valid_from(occurred_at)
                .with_sequence(sequence),
        )
        .expect("a coordinate")
        .expect("a coordinate with a scope")
    }

    fn existing() -> ExistingMemoryRefs {
        ExistingMemoryRefs {
            refs: BTreeSet::from([ABOUT.to_string(), REF.to_string()]),
            dimensions: BTreeSet::from([PROCESS.to_string(), TASK.to_string()]),
            labels: BTreeSet::from([
                ("agentic_process".to_string(), "harness".to_string()),
                ("task".to_string(), "launch".to_string()),
                ("component".to_string(), "viewer".to_string()),
            ]),
            foreign: BTreeSet::new(),
            max_sequences: BTreeMap::from([
                (("agentic_process".to_string(), PROCESS.to_string()), 4),
                (("task".to_string(), TASK.to_string()), 2),
            ]),
        }
    }

    fn current() -> Vec<TemporalCoordinate> {
        vec![
            coordinate("agentic_process", PROCESS, "2026-09-01T10:00:00Z", 3),
            coordinate("task", TASK, "2026-09-01T10:00:00Z", 2),
        ]
    }

    fn command(add: &[(&str, &str)], remove: &[(&str, &str)]) -> MemoryRelabelCommand {
        MemoryRelabelCommand {
            about: ABOUT.to_string(),
            ref_id: REF.to_string(),
            add: add.iter().map(|(key, value)| label(key, value)).collect(),
            remove: remove
                .iter()
                .map(|(key, value)| label(key, value))
                .collect(),
            why: "The decision belongs to the issue it closed.".to_string(),
            provenance: Some(MemoryProvenanceData {
                source_kind: "agent".to_string(),
                source_agent: "claude".to_string(),
                observed_at: "2026-09-05T12:00:00Z".to_string(),
                correlation_id: None,
                causation_id: None,
            }),
            idempotency_key: "relabel:test".to_string(),
            dry_run: false,
            label_policy: LabelPolicy::Warn,
            intended_new: BTreeSet::new(),
        }
    }

    #[test]
    fn an_added_label_creates_its_dimension_and_inherits_the_entry_clocks() {
        let (update, outcome) =
            translate_memory_relabel(&command(&[("issue", "506")], &[]), &existing(), &current())
                .expect("a new label translates");

        assert_eq!(
            update
                .changes
                .iter()
                .map(|change| change.entity_kind.as_str())
                .collect::<Vec<_>>(),
            ["memory_dimension", RELABEL_ENTITY_KIND]
        );
        assert_eq!(
            update.changes[0].entity_id,
            "about:project:kmp:dimension:506"
        );
        assert_eq!(update.changes[1].entity_id, REF);
        assert_eq!(
            update.changes[1].reason,
            "The decision belongs to the issue it closed."
        );
        let payload: serde_json::Value =
            serde_json::from_str(&update.changes[1].payload_json).expect("payload json");
        let added = &payload["add"][0];
        assert_eq!(added["dimension"], "issue");
        assert_eq!(added["scope_id"], "about:project:kmp:dimension:506");
        assert_eq!(
            added["occurred_at"], "2026-09-01T10:00:00Z",
            "inherited, not today"
        );
        assert_eq!(added["ingested_at"], "unix:101788000000:000000000");
        assert_eq!(added["sequence"], 1, "a counter of its own label");
        assert_eq!(payload["remove"].as_array().map(Vec::len), Some(0));
        assert_eq!(payload["actor"], "claude");
        assert_eq!(payload["observed_at"], "2026-09-05T12:00:00Z");
        assert_eq!(update.requested_by.as_deref(), Some("claude"));

        assert_eq!(outcome.added, [label("issue", "506")]);
        assert!(outcome.removed.is_empty());
        assert_eq!(
            outcome.labels,
            [
                label("agentic_process", "harness"),
                label("issue", "506"),
                label("task", "launch")
            ]
        );
        assert_eq!(
            outcome.created_dimensions,
            ["about:project:kmp:dimension:506"]
        );
        assert!(outcome.resembling_labels.is_empty());
        assert!(outcome.warnings.is_empty());
    }

    #[test]
    fn a_reused_label_declares_no_dimension_and_continues_its_counter() {
        let mut existing = existing();
        existing
            .dimensions
            .insert("about:project:kmp:dimension:viewer".to_string());
        existing.max_sequences.insert(
            (
                "component".to_string(),
                "about:project:kmp:dimension:viewer".to_string(),
            ),
            9,
        );

        let (update, outcome) = translate_memory_relabel(
            &command(&[("component", "viewer")], &[]),
            &existing,
            &current(),
        )
        .expect("a reuse translates");

        assert_eq!(update.changes.len(), 1, "no dimension declared");
        let payload: serde_json::Value =
            serde_json::from_str(&update.changes[0].payload_json).expect("payload json");
        assert_eq!(payload["add"][0]["sequence"], 10);
        assert!(outcome.created_dimensions.is_empty());
    }

    #[test]
    fn a_removed_label_names_the_edge_that_goes() {
        let (update, outcome) = translate_memory_relabel(
            &command(&[], &[("task", "launch")]),
            &existing(),
            &current(),
        )
        .expect("a removal translates");

        let payload: serde_json::Value =
            serde_json::from_str(&update.changes[0].payload_json).expect("payload json");
        assert_eq!(payload["remove"][0]["dimension"], "task");
        assert_eq!(payload["remove"][0]["scope_id"], TASK);
        assert_eq!(update.changes[0].scopes, [TASK]);
        assert_eq!(outcome.removed, [label("task", "launch")]);
        assert_eq!(outcome.labels, [label("agentic_process", "harness")]);
    }

    #[test]
    fn the_last_label_cannot_be_taken_off() {
        let error = translate_memory_relabel(
            &command(&[], &[("task", "launch"), ("agentic_process", "harness")]),
            &existing(),
            &current(),
        )
        .expect_err("an entry keeps at least one label");
        assert!(
            matches!(&error, ApplicationError::Validation(message) if message.contains("would stand in no label")),
            "{error}"
        );
    }

    #[test]
    fn a_label_the_entry_does_not_stand_in_is_refused_naming_what_it_stands_in() {
        let error =
            translate_memory_relabel(&command(&[], &[("issue", "506")]), &existing(), &current())
                .expect_err("cannot remove what is not there");
        let ApplicationError::Validation(message) = error else {
            panic!("a validation refusal: {error}");
        };
        assert!(
            message.contains("does not stand in `issue=506`"),
            "{message}"
        );
        assert!(
            message.contains("`agentic_process=harness`, `task=launch`"),
            "{message}"
        );
    }

    #[test]
    fn a_label_the_entry_already_stands_in_is_refused() {
        let error = translate_memory_relabel(
            &command(&[("task", "launch")], &[]),
            &existing(),
            &current(),
        )
        .expect_err("already there");
        assert!(
            error
                .to_string()
                .contains("already stands in `task=launch`"),
            "{error}"
        );
    }

    #[test]
    fn a_value_used_under_another_key_is_refused_naming_the_first_use() {
        let error = translate_memory_relabel(
            &command(&[("owner", "launch")], &[]),
            &existing(),
            &current(),
        )
        .expect_err("one value, one key");
        assert!(
            error
                .to_string()
                .contains("`launch` already names the label `task=launch`"),
            "{error}"
        );
    }

    #[test]
    fn a_resembling_label_is_written_and_said_under_warn_and_refused_under_refuse() {
        let (_, outcome) = translate_memory_relabel(
            &command(&[("component", "Viewer")], &[]),
            &existing(),
            &current(),
        )
        .expect("warn writes");
        assert_eq!(
            outcome.resembling_labels.len(),
            1,
            "{:?}",
            outcome.resembling_labels
        );
        assert_eq!(outcome.warnings.len(), 1);

        let mut refusing = command(&[("component", "Viewer")], &[]);
        refusing.label_policy = LabelPolicy::Refuse;
        let error = translate_memory_relabel(&refusing, &existing(), &current())
            .expect_err("refuse refuses");
        assert!(error.to_string().contains("resemble"), "{error}");

        refusing.intended_new.insert("component".to_string());
        translate_memory_relabel(&refusing, &existing(), &current())
            .expect("an intended-new key is left alone");
    }

    #[test]
    fn nothing_to_do_and_contradictory_changes_are_refused() {
        let error = translate_memory_relabel(&command(&[], &[]), &existing(), &current())
            .expect_err("nothing to relabel");
        assert!(error.to_string().contains("nothing to relabel"), "{error}");

        let error = translate_memory_relabel(
            &command(&[("task", "launch")], &[("task", "launch")]),
            &existing(),
            &current(),
        )
        .expect_err("both added and removed");
        assert!(
            error.to_string().contains("both added and removed"),
            "{error}"
        );
    }

    #[test]
    fn a_memory_the_about_does_not_hold_is_not_found() {
        let mut existing = existing();
        existing.refs.remove(REF);
        let error =
            translate_memory_relabel(&command(&[("issue", "506")], &[]), &existing, &current())
                .expect_err("not found");
        assert!(matches!(error, ApplicationError::NotFound(_)), "{error}");
    }

    #[test]
    fn the_logical_digest_reads_what_the_caller_said() {
        let (first, _) =
            translate_memory_relabel(&command(&[("issue", "506")], &[]), &existing(), &current())
                .expect("translates");
        let (again, _) =
            translate_memory_relabel(&command(&[("issue", "506")], &[]), &existing(), &current())
                .expect("translates");
        let (other, _) =
            translate_memory_relabel(&command(&[("issue", "507")], &[]), &existing(), &current())
                .expect("translates");
        assert_eq!(first.logical_digest, again.logical_digest);
        assert_ne!(first.logical_digest, other.logical_digest);
    }
}
