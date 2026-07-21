use std::collections::BTreeMap;
use std::time::{Duration, SystemTime};

use rehydration_domain::{
    NodeDetailProjection, NodeProjection, PortError, ProjectionCheckpoint, Provenance,
    RelationExplanation, SourceKind,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub(crate) fn encode<T: Serialize>(what: &str, value: &T) -> Result<Vec<u8>, PortError> {
    serde_json::to_vec(value).map_err(|error| {
        PortError::InvalidState(format!("embedded store could not encode {what}: {error}"))
    })
}

pub(crate) fn decode<T: DeserializeOwned>(what: &str, bytes: &[u8]) -> Result<T, PortError> {
    serde_json::from_slice(bytes).map_err(|error| {
        PortError::InvalidState(format!("embedded store could not decode {what}: {error}"))
    })
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct NodeRecord {
    node_id: String,
    node_kind: String,
    title: String,
    summary: String,
    status: String,
    labels: Vec<String>,
    properties: BTreeMap<String, String>,
    source_kind: Option<String>,
    source_agent: Option<String>,
    observed_at: Option<String>,
}

impl From<NodeProjection> for NodeRecord {
    fn from(node: NodeProjection) -> Self {
        let (source_kind, source_agent, observed_at) = match node.provenance {
            Some(ref provenance) => (
                Some(provenance.source_kind().as_str().to_string()),
                provenance.source_agent().map(str::to_string),
                provenance.observed_at().map(str::to_string),
            ),
            None => (None, None, None),
        };
        Self {
            node_id: node.node_id,
            node_kind: node.node_kind,
            title: node.title,
            summary: node.summary,
            status: node.status,
            labels: node.labels,
            properties: node.properties,
            source_kind,
            source_agent,
            observed_at,
        }
    }
}

impl NodeRecord {
    pub(crate) fn into_projection(self) -> Result<NodeProjection, PortError> {
        let provenance = match self.source_kind {
            Some(kind) => {
                let kind = SourceKind::parse(&kind).map_err(|error| {
                    PortError::InvalidState(format!(
                        "embedded store node `{}` has invalid provenance source kind: {error}",
                        self.node_id
                    ))
                })?;
                let mut provenance = Provenance::new(kind);
                if let Some(agent) = self.source_agent {
                    provenance = provenance.with_source_agent(agent);
                }
                if let Some(observed_at) = self.observed_at {
                    provenance = provenance.with_observed_at(observed_at);
                }
                Some(provenance)
            }
            None => None,
        };
        Ok(NodeProjection {
            node_id: self.node_id,
            node_kind: self.node_kind,
            title: self.title,
            summary: self.summary,
            status: self.status,
            labels: self.labels,
            properties: self.properties,
            provenance,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct DetailRecord {
    node_id: String,
    detail: String,
    content_hash: String,
    revision: u64,
}

impl From<NodeDetailProjection> for DetailRecord {
    fn from(detail: NodeDetailProjection) -> Self {
        Self {
            node_id: detail.node_id,
            detail: detail.detail,
            content_hash: detail.content_hash,
            revision: detail.revision,
        }
    }
}

impl From<DetailRecord> for NodeDetailProjection {
    fn from(record: DetailRecord) -> Self {
        Self {
            node_id: record.node_id,
            detail: record.detail,
            content_hash: record.content_hash,
            revision: record.revision,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct AggregateRecord {
    pub(crate) revision: u64,
    pub(crate) content_hash: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CheckpointRecord {
    consumer_name: String,
    stream_name: String,
    last_subject: String,
    last_event_id: String,
    last_correlation_id: String,
    last_occurred_at: String,
    processed_events: u64,
    updated_at_millis: u64,
}

impl From<ProjectionCheckpoint> for CheckpointRecord {
    fn from(checkpoint: ProjectionCheckpoint) -> Self {
        let updated_at_millis = checkpoint
            .updated_at
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0);
        Self {
            consumer_name: checkpoint.consumer_name,
            stream_name: checkpoint.stream_name,
            last_subject: checkpoint.last_subject,
            last_event_id: checkpoint.last_event_id,
            last_correlation_id: checkpoint.last_correlation_id,
            last_occurred_at: checkpoint.last_occurred_at,
            processed_events: checkpoint.processed_events,
            updated_at_millis,
        }
    }
}

impl From<CheckpointRecord> for ProjectionCheckpoint {
    fn from(record: CheckpointRecord) -> Self {
        Self {
            consumer_name: record.consumer_name,
            stream_name: record.stream_name,
            last_subject: record.last_subject,
            last_event_id: record.last_event_id,
            last_correlation_id: record.last_correlation_id,
            last_occurred_at: record.last_occurred_at,
            processed_events: record.processed_events,
            updated_at: SystemTime::UNIX_EPOCH + Duration::from_millis(record.updated_at_millis),
        }
    }
}

pub(crate) fn encode_explanation(explanation: &RelationExplanation) -> Result<Vec<u8>, PortError> {
    encode("relation explanation", &explanation.to_properties())
}

pub(crate) fn decode_explanation(bytes: &[u8]) -> Result<RelationExplanation, PortError> {
    let properties: BTreeMap<String, String> = decode("relation explanation", bytes)?;
    RelationExplanation::from_properties(&properties).map_err(|error| {
        PortError::InvalidState(format!(
            "embedded store relation explanation could not be rebuilt: {error}"
        ))
    })
}
