use kmp_domain::{
    GraphNodeMaterializedEvent, GraphRelationMaterializedEvent, NodeDetailMaterializedEvent,
    ProjectionEvent,
};

use super::error::NatsConsumerError;
use super::subject_routing::ProjectionSubject;

pub(crate) fn decode_projection_event(
    subject: ProjectionSubject,
    payload: &[u8],
) -> Result<ProjectionEvent, NatsConsumerError> {
    match subject {
        ProjectionSubject::GraphNode => {
            serde_json::from_slice::<GraphNodeMaterializedEvent>(payload)
                .map(ProjectionEvent::GraphNodeMaterialized)
                .map_err(|error| NatsConsumerError::InvalidPayload(error.to_string()))
        }
        ProjectionSubject::GraphRelation => {
            serde_json::from_slice::<GraphRelationMaterializedEvent>(payload)
                .map(ProjectionEvent::GraphRelationMaterialized)
                .map_err(|error| NatsConsumerError::InvalidPayload(error.to_string()))
        }
        ProjectionSubject::NodeDetail => {
            serde_json::from_slice::<NodeDetailMaterializedEvent>(payload)
                .map(ProjectionEvent::NodeDetailMaterialized)
                .map_err(|error| NatsConsumerError::InvalidPayload(error.to_string()))
        }
    }
}
