use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionCheckpoint {
    pub consumer_name: String,
    pub stream_name: String,
    pub last_subject: String,
    pub last_event_id: String,
    pub last_correlation_id: String,
    pub last_occurred_at: String,
    pub processed_events: u64,
    pub updated_at: SystemTime,
}
