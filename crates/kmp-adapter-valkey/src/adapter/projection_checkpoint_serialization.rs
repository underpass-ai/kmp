use std::time::{Duration, SystemTime, UNIX_EPOCH};

use kmp_ports::{PortError, ProjectionCheckpoint};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct RawProjectionCheckpoint {
    consumer_name: String,
    stream_name: String,
    last_subject: String,
    last_event_id: String,
    last_correlation_id: String,
    last_occurred_at: String,
    processed_events: u64,
    updated_at_ms: u64,
}

pub(crate) fn serialize_projection_checkpoint(
    checkpoint: &ProjectionCheckpoint,
) -> Result<String, PortError> {
    serde_json::to_string(&RawProjectionCheckpoint {
        consumer_name: checkpoint.consumer_name.clone(),
        stream_name: checkpoint.stream_name.clone(),
        last_subject: checkpoint.last_subject.clone(),
        last_event_id: checkpoint.last_event_id.clone(),
        last_correlation_id: checkpoint.last_correlation_id.clone(),
        last_occurred_at: checkpoint.last_occurred_at.clone(),
        processed_events: checkpoint.processed_events,
        updated_at_ms: system_time_to_millis(checkpoint.updated_at),
    })
    .map_err(|error| {
        PortError::InvalidState(format!(
            "projection checkpoint could not be serialized for valkey: {error}"
        ))
    })
}

pub(crate) fn deserialize_projection_checkpoint(
    payload: &str,
) -> Result<Option<ProjectionCheckpoint>, PortError> {
    serde_json::from_str::<RawProjectionCheckpoint>(payload)
        .map(|checkpoint| {
            Some(ProjectionCheckpoint {
                consumer_name: checkpoint.consumer_name,
                stream_name: checkpoint.stream_name,
                last_subject: checkpoint.last_subject,
                last_event_id: checkpoint.last_event_id,
                last_correlation_id: checkpoint.last_correlation_id,
                last_occurred_at: checkpoint.last_occurred_at,
                processed_events: checkpoint.processed_events,
                updated_at: UNIX_EPOCH + Duration::from_millis(checkpoint.updated_at_ms),
            })
        })
        .map_err(|error| {
            PortError::InvalidState(format!(
                "projection checkpoint could not be deserialized from valkey: {error}"
            ))
        })
}

fn system_time_to_millis(value: SystemTime) -> u64 {
    value
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use kmp_ports::ProjectionCheckpoint;

    use super::{deserialize_projection_checkpoint, serialize_projection_checkpoint};

    #[test]
    fn projection_checkpoint_roundtrip_preserves_all_fields() {
        let checkpoint = ProjectionCheckpoint {
            consumer_name: "projection-consumer".to_string(),
            stream_name: "graph.node.materialized".to_string(),
            last_subject: "rehydration.graph.node.materialized".to_string(),
            last_event_id: "event-1".to_string(),
            last_correlation_id: "corr-1".to_string(),
            last_occurred_at: "2026-03-12T00:00:00Z".to_string(),
            processed_events: 7,
            updated_at: UNIX_EPOCH + Duration::from_millis(1234),
        };

        let payload =
            serialize_projection_checkpoint(&checkpoint).expect("checkpoint should serialize");
        let decoded = deserialize_projection_checkpoint(&payload)
            .expect("checkpoint should deserialize")
            .expect("checkpoint should be present");

        assert_eq!(decoded, checkpoint);
    }
}
