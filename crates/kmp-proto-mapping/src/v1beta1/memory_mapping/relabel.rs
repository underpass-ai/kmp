use kmp_application::{EntryLabelData, LabelPolicy, MemoryRelabelCommand, MemoryRelabelOutcome};
use kmp_proto::v1beta1::{
    EntryLabel, LabelPolicy as ProtoLabelPolicy, RelabelRequest, RelabelResponse, RelabelledMemory,
    ResemblingLabel,
};

use super::ingest::provenance_from_proto;
use super::scalars::plural;

pub fn relabel_command_from_proto(request: RelabelRequest) -> MemoryRelabelCommand {
    MemoryRelabelCommand {
        about: request.about,
        ref_id: request.r#ref,
        add: request.add.into_iter().map(label_from_proto).collect(),
        remove: request.remove.into_iter().map(label_from_proto).collect(),
        why: request.why,
        provenance: request.provenance.map(provenance_from_proto),
        idempotency_key: request.idempotency_key,
        dry_run: request.dry_run,
        label_policy: match ProtoLabelPolicy::try_from(request.label_policy) {
            Ok(ProtoLabelPolicy::Refuse) => LabelPolicy::Refuse,
            _ => LabelPolicy::Warn,
        },
        intended_new: request.intended_new.into_iter().collect(),
    }
}

pub fn relabel_response_from_outcome(outcome: MemoryRelabelOutcome) -> RelabelResponse {
    RelabelResponse {
        summary: format!(
            "Relabelled `{}` in {}: added {} {}, removed {}; it stands in {} {}.",
            outcome.ref_id,
            outcome.about,
            outcome.added.len(),
            plural(outcome.added.len(), "label", "labels"),
            outcome.removed.len(),
            outcome.labels.len(),
            plural(outcome.labels.len(), "label", "labels"),
        ),
        memory: Some(RelabelledMemory {
            about: outcome.about,
            r#ref: outcome.ref_id,
            added: outcome.added.into_iter().map(label_to_proto).collect(),
            removed: outcome.removed.into_iter().map(label_to_proto).collect(),
            labels: outcome.labels.into_iter().map(label_to_proto).collect(),
            created_dimensions: outcome.created_dimensions,
            resembling_labels: outcome
                .resembling_labels
                .into_iter()
                .map(|label| ResemblingLabel {
                    key: label.key,
                    value: label.value,
                    existing_key: label.existing_key,
                    existing_value: label.existing_value,
                    kind: label.kind,
                    why: label.why,
                })
                .collect(),
            read_after_write_ready: outcome.read_after_write_ready,
        }),
        warnings: outcome.warnings,
    }
}

fn label_from_proto(label: EntryLabel) -> EntryLabelData {
    EntryLabelData {
        key: label.key,
        value: label.value,
    }
}

fn label_to_proto(label: EntryLabelData) -> EntryLabel {
    EntryLabel {
        key: label.key,
        value: label.value,
    }
}

#[cfg(test)]
mod tests {
    use kmp_application::{EntryLabelData, LabelPolicy, MemoryRelabelOutcome};
    use kmp_proto::v1beta1::{EntryLabel, LabelPolicy as ProtoLabelPolicy, RelabelRequest};

    use super::{relabel_command_from_proto, relabel_response_from_outcome};

    fn label(key: &str, value: &str) -> EntryLabelData {
        EntryLabelData {
            key: key.to_string(),
            value: value.to_string(),
        }
    }

    #[test]
    fn the_request_maps_field_for_field_and_reads_the_policy() {
        let command = relabel_command_from_proto(RelabelRequest {
            about: "service:alpha".to_string(),
            r#ref: "service:alpha:decision:canary".to_string(),
            add: vec![EntryLabel {
                key: "issue".to_string(),
                value: "506".to_string(),
            }],
            remove: vec![],
            why: "It closed the issue.".to_string(),
            provenance: None,
            idempotency_key: "relabel:1".to_string(),
            dry_run: true,
            label_policy: ProtoLabelPolicy::Refuse as i32,
            intended_new: vec!["issue".to_string()],
        });

        assert_eq!(command.ref_id, "service:alpha:decision:canary");
        assert_eq!(command.add, [label("issue", "506")]);
        assert!(command.remove.is_empty());
        assert!(command.dry_run);
        assert_eq!(command.label_policy, LabelPolicy::Refuse);
        assert!(command.intended_new.contains("issue"));
    }

    #[test]
    fn the_response_counts_what_changed_and_what_stands() {
        let response = relabel_response_from_outcome(MemoryRelabelOutcome {
            about: "service:alpha".to_string(),
            ref_id: "service:alpha:decision:canary".to_string(),
            added: vec![label("issue", "506")],
            removed: vec![],
            labels: vec![label("issue", "506"), label("release", "spring")],
            created_dimensions: vec!["about:service:alpha:dimension:506".to_string()],
            resembling_labels: vec![],
            read_after_write_ready: true,
            warnings: vec![],
        });

        assert_eq!(
            response.summary,
            "Relabelled `service:alpha:decision:canary` in service:alpha: added 1 label, removed 0; it stands in 2 labels."
        );
        let memory = response.memory.expect("memory");
        assert_eq!(memory.labels.len(), 2);
        assert_eq!(memory.created_dimensions.len(), 1);
        assert!(memory.read_after_write_ready);
    }
}
