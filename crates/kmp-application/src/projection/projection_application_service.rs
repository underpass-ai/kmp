use kmp_domain::{
    NodeDetailProjection, NodeProjection, NodeRelationProjection, PortError, ProcessedEventStore,
    ProjectionCheckpoint, ProjectionCheckpointStore, ProjectionEvent, ProjectionEventHandler,
    ProjectionHandlingRequest, ProjectionHandlingResult, ProjectionMutation, ProjectionWriter,
};

#[derive(Debug)]
pub struct ProjectionApplicationService<W, P, C> {
    projection_writer: W,
    processed_event_store: P,
    checkpoint_store: C,
}

impl<W, P, C> ProjectionApplicationService<W, P, C>
where
    W: ProjectionWriter + Send + Sync,
    P: ProcessedEventStore + Send + Sync,
    C: ProjectionCheckpointStore + Send + Sync,
{
    pub fn new(projection_writer: W, processed_event_store: P, checkpoint_store: C) -> Self {
        Self {
            projection_writer,
            processed_event_store,
            checkpoint_store,
        }
    }

    async fn mutations_for_event(
        &self,
        event: &ProjectionEvent,
    ) -> Result<Vec<ProjectionMutation>, PortError> {
        Ok(match event {
            ProjectionEvent::GraphNodeMaterialized(event) => {
                let provenance = event
                    .data
                    .source_kind
                    .as_deref()
                    .and_then(|sk| kmp_domain::SourceKind::parse(sk).ok())
                    .map(|sk| {
                        let mut p = kmp_domain::Provenance::new(sk);
                        if let Some(ref agent) = event.data.source_agent {
                            p = p.with_source_agent(agent.clone());
                        }
                        if let Some(ref observed) = event.data.observed_at {
                            p = p.with_observed_at(observed.clone());
                        }
                        p
                    });
                let mut mutations = vec![ProjectionMutation::UpsertNode(NodeProjection {
                    node_id: event.data.node_id.clone(),
                    node_kind: event.data.node_kind.clone(),
                    title: event.data.title.clone(),
                    summary: event.data.summary.clone(),
                    status: event.data.status.clone(),
                    labels: event.data.labels.clone(),
                    properties: event.data.properties.clone(),
                    provenance,
                })];
                mutations.extend(
                    event
                        .data
                        .related_nodes
                        .iter()
                        .map(|reference| {
                            Ok(ProjectionMutation::UpsertNodeRelation(Box::new(
                                NodeRelationProjection {
                        source_node_id: event.data.node_id.clone(),
                        target_node_id: reference.node_id.clone(),
                        relation_type: reference.relation_type.clone(),
                        explanation: reference.explanation.clone().try_into().map_err(|error| {
                            PortError::InvalidState(format!(
                                "invalid related node explanation for `{}` -> `{}`: {error}",
                                event.data.node_id, reference.node_id
                            ))
                        })?,
                    },
                            )))
                        })
                        .collect::<Result<Vec<_>, PortError>>()?,
                );
                mutations
            }
            ProjectionEvent::GraphRelationMaterialized(event) => {
                vec![ProjectionMutation::UpsertNodeRelation(Box::new(
                    NodeRelationProjection {
                        source_node_id: event.data.source_node_id.clone(),
                        target_node_id: event.data.target_node_id.clone(),
                        relation_type: event.data.relation_type.clone(),
                        explanation: event.data.explanation.clone().try_into().map_err(
                            |error| {
                                PortError::InvalidState(format!(
                                    "invalid relation explanation for `{}` -> `{}`: {error}",
                                    event.data.source_node_id, event.data.target_node_id
                                ))
                            },
                        )?,
                    },
                ))]
            }
            ProjectionEvent::NodeDetailMaterialized(event) => {
                vec![ProjectionMutation::UpsertNodeDetail(NodeDetailProjection {
                    node_id: event.data.node_id.clone(),
                    detail: event.data.detail.clone(),
                    content_hash: event.data.content_hash.clone(),
                    revision: event.data.revision,
                })]
            }
        })
    }
}

impl<W, P, C> ProjectionEventHandler for ProjectionApplicationService<W, P, C>
where
    W: ProjectionWriter + Send + Sync,
    P: ProcessedEventStore + Send + Sync,
    C: ProjectionCheckpointStore + Send + Sync,
{
    async fn handle_projection_event(
        &self,
        request: ProjectionHandlingRequest,
    ) -> Result<ProjectionHandlingResult, PortError> {
        let event_id = request.event.event_id().to_string();
        if self
            .processed_event_store
            .has_processed(&request.consumer_name, &event_id)
            .await?
        {
            return Ok(ProjectionHandlingResult {
                event_id,
                subject: request.subject,
                duplicate: true,
                applied_mutations: 0,
                checkpoint: None,
            });
        }

        let mutations = self.mutations_for_event(&request.event).await?;
        self.projection_writer
            .apply_mutations(mutations.clone())
            .await?;
        self.processed_event_store
            .record_processed(&request.consumer_name, &event_id)
            .await?;
        let checkpoint = ProjectionCheckpoint {
            consumer_name: request.consumer_name,
            stream_name: request.stream_name,
            last_subject: request.subject.clone(),
            last_event_id: event_id.clone(),
            last_correlation_id: request.event.envelope().correlation_id.clone(),
            last_occurred_at: request.event.envelope().occurred_at.clone(),
            processed_events: 1,
            updated_at: std::time::SystemTime::now(),
        };
        self.checkpoint_store
            .save_checkpoint(checkpoint.clone())
            .await?;

        Ok(ProjectionHandlingResult {
            event_id,
            subject: request.subject,
            duplicate: false,
            applied_mutations: mutations.len(),
            checkpoint: Some(checkpoint),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kmp_domain::{
        GraphRelationMaterializedData, GraphRelationMaterializedEvent, PortError,
        ProcessedEventStore, ProjectionCheckpoint, ProjectionCheckpointStore, ProjectionEnvelope,
        ProjectionEvent, ProjectionEventHandler, ProjectionHandlingRequest, ProjectionMutation,
        ProjectionWriter, RelatedNodeExplanationData, RelationSemanticClass,
    };
    use tokio::sync::Mutex;

    use super::ProjectionApplicationService;

    #[derive(Debug, Default, Clone)]
    struct RecordingProjectionWriter {
        mutations: Arc<Mutex<Vec<ProjectionMutation>>>,
    }

    impl RecordingProjectionWriter {
        async fn mutations(&self) -> Vec<ProjectionMutation> {
            self.mutations.lock().await.clone()
        }
    }

    impl ProjectionWriter for RecordingProjectionWriter {
        async fn apply_mutations(
            &self,
            mutations: Vec<ProjectionMutation>,
        ) -> Result<(), PortError> {
            self.mutations.lock().await.extend(mutations);
            Ok(())
        }
    }

    #[derive(Debug, Default, Clone)]
    struct RecordingProcessedEventStore {
        processed: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl ProcessedEventStore for RecordingProcessedEventStore {
        async fn has_processed(
            &self,
            consumer_name: &str,
            event_id: &str,
        ) -> Result<bool, PortError> {
            Ok(self
                .processed
                .lock()
                .await
                .iter()
                .any(|(consumer, event)| consumer == consumer_name && event == event_id))
        }

        async fn record_processed(
            &self,
            consumer_name: &str,
            event_id: &str,
        ) -> Result<(), PortError> {
            self.processed
                .lock()
                .await
                .push((consumer_name.to_string(), event_id.to_string()));
            Ok(())
        }
    }

    #[derive(Debug, Default, Clone)]
    struct RecordingCheckpointStore {
        checkpoints: Arc<Mutex<Vec<ProjectionCheckpoint>>>,
    }

    impl ProjectionCheckpointStore for RecordingCheckpointStore {
        async fn save_checkpoint(&self, checkpoint: ProjectionCheckpoint) -> Result<(), PortError> {
            self.checkpoints.lock().await.push(checkpoint);
            Ok(())
        }

        async fn load_checkpoint(
            &self,
            _consumer_name: &str,
            _stream_name: &str,
        ) -> Result<Option<ProjectionCheckpoint>, PortError> {
            Ok(self.checkpoints.lock().await.last().cloned())
        }
    }

    #[tokio::test]
    async fn relation_materialized_event_upserts_relation_directly() {
        let writer = RecordingProjectionWriter::default();
        let service = ProjectionApplicationService::new(
            writer.clone(),
            RecordingProcessedEventStore::default(),
            RecordingCheckpointStore::default(),
        );

        let request = ProjectionHandlingRequest {
            consumer_name: "projection-consumer".to_string(),
            stream_name: "rehydration.events".to_string(),
            subject: "graph.relation.materialized".to_string(),
            event: ProjectionEvent::GraphRelationMaterialized(GraphRelationMaterializedEvent {
                envelope: ProjectionEnvelope {
                    event_id: "evt-relation-1".to_string(),
                    correlation_id: "corr-1".to_string(),
                    causation_id: "cmd-1".to_string(),
                    occurred_at: "2026-04-14T18:45:00Z".to_string(),
                    aggregate_id: "relation:decision|addresses|finding".to_string(),
                    aggregate_type: "node_relation".to_string(),
                    schema_version: "v1beta1".to_string(),
                },
                data: GraphRelationMaterializedData {
                    source_node_id: "decision-1".to_string(),
                    target_node_id: "finding-1".to_string(),
                    relation_type: "addresses".to_string(),
                    explanation: RelatedNodeExplanationData {
                        semantic_class: RelationSemanticClass::Causal,
                        rationale: Some("decision addresses finding".to_string()),
                        motivation: None,
                        method: None,
                        decision_id: Some("decision-1".to_string()),
                        caused_by_node_id: None,
                        evidence: None,
                        confidence: Some("high".to_string()),
                        sequence: Some(2),
                    },
                },
            }),
        };

        let result = service
            .handle_projection_event(request)
            .await
            .expect("relation event should apply");

        assert_eq!(result.subject, "graph.relation.materialized");
        assert_eq!(result.applied_mutations, 1);

        let mutations = writer.mutations().await;
        assert_eq!(mutations.len(), 1);
        match &mutations[0] {
            ProjectionMutation::UpsertNodeRelation(relation) => {
                assert_eq!(relation.source_node_id, "decision-1");
                assert_eq!(relation.target_node_id, "finding-1");
                assert_eq!(relation.relation_type, "addresses");
            }
            mutation => panic!("unexpected mutation: {mutation:?}"),
        }
    }
}
