use async_nats::jetstream;
use async_nats::jetstream::stream;

use rehydration_domain::{ContextEventStore, ContextUpdatedEvent, IdempotentOutcome, PortError};

const STREAM_NAME: &str = "CONTEXT_EVENTS";

/// NATS JetStream implementation of the context event store.
///
/// Events are persisted as JetStream messages. Revision tracking uses the
/// stream sequence number. Idempotency is tracked via a separate KV-style
/// subject per key.
pub struct NatsContextEventStore {
    js: jetstream::Context,
    subject_prefix: String,
}

impl std::fmt::Debug for NatsContextEventStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NatsContextEventStore")
            .field("subject_prefix", &self.subject_prefix)
            .finish_non_exhaustive()
    }
}

impl NatsContextEventStore {
    pub async fn new(
        client: async_nats::Client,
        subject_prefix: impl Into<String>,
    ) -> Result<Self, PortError> {
        let js = jetstream::new(client);
        let prefix = subject_prefix.into();
        let subjects = vec![format!("{prefix}.cmd.>")];
        js.get_or_create_stream(stream::Config {
            name: STREAM_NAME.to_string(),
            subjects,
            retention: stream::RetentionPolicy::Limits,
            storage: stream::StorageType::File,
            ..Default::default()
        })
        .await
        .map_err(|error| {
            PortError::Unavailable(format!("jetstream stream setup failed: {error}"))
        })?;

        Ok(Self {
            js,
            subject_prefix: prefix,
        })
    }

    fn event_subject(&self, root_node_id: &str, role: &str) -> String {
        format!("{}.cmd.evt.{}.{}", self.subject_prefix, root_node_id, role)
    }

    fn idem_subject(&self, key: &str) -> String {
        format!("{}.cmd.idem.{}", self.subject_prefix, key)
    }

    /// Returns `(domain_revision, content_hash, stream_sequence)`.
    /// `stream_sequence` is the JetStream sequence number used for CAS.
    async fn last_event_for(
        &self,
        root_node_id: &str,
        role: &str,
    ) -> Result<Option<(u64, String, u64)>, PortError> {
        let subject = self.event_subject(root_node_id, role);
        let stream = self
            .js
            .get_stream(STREAM_NAME)
            .await
            .map_err(|error| PortError::Unavailable(format!("get stream failed: {error}")))?;

        match stream.get_last_raw_message_by_subject(&subject).await {
            Ok(msg) => {
                let seq = msg.sequence;
                let event: ContextUpdatedEvent =
                    serde_json::from_slice(&msg.payload).map_err(|error| {
                        PortError::Unavailable(format!(
                            "failed to deserialize context event: {error}"
                        ))
                    })?;
                Ok(Some((event.revision, event.content_hash, seq)))
            }
            Err(error) => {
                if matches!(
                    error.kind(),
                    async_nats::jetstream::stream::LastRawMessageErrorKind::NoMessageFound
                ) {
                    Ok(None)
                } else {
                    Err(PortError::Unavailable(format!(
                        "get last message failed: {error}"
                    )))
                }
            }
        }
    }
}

impl ContextEventStore for NatsContextEventStore {
    async fn append(
        &self,
        event: ContextUpdatedEvent,
        expected_revision: u64,
    ) -> Result<u64, PortError> {
        let last = self
            .last_event_for(&event.root_node_id, &event.role)
            .await?;
        let (current_revision, last_sequence) = match last {
            Some((rev, _, seq)) => (rev, seq),
            None => (0, 0),
        };
        if current_revision != expected_revision {
            return Err(PortError::Conflict(format!(
                "expected revision {expected_revision}, current is {current_revision}"
            )));
        }

        let new_revision = current_revision + 1;
        let subject = self.event_subject(&event.root_node_id, &event.role);

        // Persist the full event as JSON with CAS via expected_last_subject_sequence.
        // JetStream rejects the publish if another client appended since our read.
        let payload = serde_json::to_vec(&event).map_err(|error| {
            PortError::Unavailable(format!("failed to serialize context event: {error}"))
        })?;

        let mut headers = async_nats::HeaderMap::new();
        headers.insert(
            async_nats::header::NATS_EXPECTED_LAST_SUBJECT_SEQUENCE,
            async_nats::HeaderValue::from(last_sequence),
        );

        match self
            .js
            .publish_with_headers(subject, headers, payload.into())
            .await
        {
            Ok(ack_future) => {
                ack_future.await.map_err(|error| {
                    let msg = error.to_string();
                    if msg.contains("wrong last subject sequence") {
                        PortError::Conflict(format!(
                            "concurrent write detected (CAS rejected): {msg}"
                        ))
                    } else {
                        PortError::Unavailable(format!("publish ack failed: {msg}"))
                    }
                })?;
            }
            Err(error) => {
                return Err(PortError::Unavailable(format!(
                    "publish event failed: {error}"
                )));
            }
        }

        if let Some(ref idem_key) = event.idempotency_key {
            let idem_subject = self.idem_subject(idem_key);
            let idem_outcome = IdempotentOutcome {
                revision: new_revision,
                content_hash: event.content_hash.clone(),
                logical_digest: event.logical_digest.clone(),
            };
            let idem_payload = serde_json::to_vec(&idem_outcome).map_err(|error| {
                PortError::Unavailable(format!("failed to serialize idempotent outcome: {error}"))
            })?;
            match self.js.publish(idem_subject, idem_payload.into()).await {
                Ok(ack_future) => {
                    if let Err(error) = ack_future.await {
                        tracing::warn!(
                            idempotency_key = idem_key.as_str(),
                            %error,
                            "idempotency outcome ack failed — retries may be treated as new requests"
                        );
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        idempotency_key = idem_key.as_str(),
                        %error,
                        "idempotency outcome publish failed — retries may be treated as new requests"
                    );
                }
            }
        }

        Ok(new_revision)
    }

    async fn current_revision(&self, root_node_id: &str, role: &str) -> Result<u64, PortError> {
        match self.last_event_for(root_node_id, role).await? {
            Some((revision, _, _)) => Ok(revision),
            None => Ok(0),
        }
    }

    async fn current_content_hash(
        &self,
        root_node_id: &str,
        role: &str,
    ) -> Result<Option<String>, PortError> {
        match self.last_event_for(root_node_id, role).await? {
            Some((_, hash, _)) if !hash.is_empty() => Ok(Some(hash)),
            _ => Ok(None),
        }
    }

    async fn find_by_idempotency_key(
        &self,
        key: &str,
    ) -> Result<Option<IdempotentOutcome>, PortError> {
        let subject = self.idem_subject(key);
        let stream = self
            .js
            .get_stream(STREAM_NAME)
            .await
            .map_err(|error| PortError::Unavailable(format!("get stream failed: {error}")))?;

        match stream.get_last_raw_message_by_subject(&subject).await {
            Ok(msg) => {
                let outcome: IdempotentOutcome =
                    serde_json::from_slice(&msg.payload).map_err(|error| {
                        PortError::Unavailable(format!(
                            "failed to deserialize idempotent outcome: {error}"
                        ))
                    })?;
                Ok(Some(outcome))
            }
            Err(error) => {
                if matches!(
                    error.kind(),
                    async_nats::jetstream::stream::LastRawMessageErrorKind::NoMessageFound
                ) {
                    Ok(None)
                } else {
                    Err(PortError::Unavailable(format!(
                        "idempotency lookup failed: {error}"
                    )))
                }
            }
        }
    }
}
