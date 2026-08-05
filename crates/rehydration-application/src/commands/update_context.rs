use std::sync::Arc;
use std::time::SystemTime;

use rehydration_domain::{
    CaseId, ContextEventChange, ContextEventStore, ContextUpdatedEvent, PortError,
    ProjectionMutation, ProjectionWriter, Role,
};

use crate::ApplicationError;
use crate::commands::memory_projection::memory_projection_mutations;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateContextChange {
    pub operation: String,
    pub entity_kind: String,
    pub entity_id: String,
    pub payload_json: String,
    pub reason: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateContextCommand {
    pub root_node_id: String,
    pub role: String,
    pub work_item_id: String,
    pub changes: Vec<UpdateContextChange>,
    pub expected_revision: Option<u64>,
    pub expected_content_hash: Option<String>,
    pub idempotency_key: Option<String>,
    /// Digest of the logical command, computed by the caller *before* any
    /// state-dependent translation. Optional: a caller that supplies none gets
    /// idempotency compared on the translated changes, which refuses a replay
    /// whenever translation saw different state — safe, but stricter than the
    /// caller may mean.
    pub logical_digest: Option<String>,
    pub requested_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedVersion {
    pub revision: u64,
    pub content_hash: String,
    pub generator_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateContextOutcome {
    pub accepted_version: AcceptedVersion,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub struct UpdateContextUseCase<E, W = NoopProjectionWriter> {
    event_store: Arc<E>,
    projection_writer: W,
    generator_version: &'static str,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoopProjectionWriter;

impl ProjectionWriter for NoopProjectionWriter {
    async fn apply_mutations(&self, _mutations: Vec<ProjectionMutation>) -> Result<(), PortError> {
        Ok(())
    }
}

impl<E> UpdateContextUseCase<E, NoopProjectionWriter>
where
    E: ContextEventStore + Send + Sync,
{
    pub fn new(event_store: Arc<E>, generator_version: &'static str) -> Self {
        Self {
            event_store,
            projection_writer: NoopProjectionWriter,
            generator_version,
        }
    }
}

impl<E, W> UpdateContextUseCase<E, W>
where
    E: ContextEventStore + Send + Sync,
    W: ProjectionWriter + Send + Sync,
{
    pub fn new_with_projection_writer(
        event_store: Arc<E>,
        projection_writer: W,
        generator_version: &'static str,
    ) -> Self {
        Self {
            event_store,
            projection_writer,
            generator_version,
        }
    }

    pub async fn execute(
        &self,
        command: UpdateContextCommand,
    ) -> Result<UpdateContextOutcome, ApplicationError> {
        let case_id = CaseId::new(&command.root_node_id)?;
        let role = Role::new(&command.role)?;

        // Compute content hash from actual change payloads. Duplicate idempotency keys
        // must replay the same logical command, not mask a conflicting payload.
        let content_hash = compute_content_hash(&command.changes);

        // Idempotency check. The logical digest, when both sides carry one,
        // is the honest comparison: the same logical command translated after
        // its own first apply produces different changes (creates become
        // updates), and comparing those would refuse a legitimate replay.
        if let Some(ref key) = command.idempotency_key
            && let Some(outcome) = self.event_store.find_by_idempotency_key(key).await?
        {
            let same_logical_command = match (&command.logical_digest, &outcome.logical_digest) {
                (Some(ours), Some(stored)) => ours == stored,
                _ => outcome.content_hash == content_hash,
            };
            if !same_logical_command {
                return Err(ApplicationError::Ports(
                    rehydration_domain::PortError::Conflict(format!(
                        "idempotency key '{key}' was already accepted with different content"
                    )),
                ));
            }
            return Ok(UpdateContextOutcome {
                accepted_version: AcceptedVersion {
                    revision: outcome.revision,
                    content_hash: outcome.content_hash,
                    generator_version: self.generator_version.to_string(),
                },
                warnings: vec![],
            });
        }

        // Load current revision
        let current_revision = self
            .event_store
            .current_revision(case_id.as_str(), role.as_str())
            .await?;

        // Validate revision precondition
        let expected_revision = command.expected_revision.unwrap_or(current_revision);
        if expected_revision != current_revision {
            return Err(ApplicationError::Ports(
                rehydration_domain::PortError::Conflict(format!(
                    "expected revision {expected_revision}, current is {current_revision}"
                )),
            ));
        }

        // Validate content hash precondition
        if let Some(ref expected_hash) = command.expected_content_hash
            && let Some(ref current_hash) = self
                .event_store
                .current_content_hash(case_id.as_str(), role.as_str())
                .await?
            && expected_hash != current_hash
        {
            return Err(ApplicationError::Ports(
                rehydration_domain::PortError::Conflict(format!(
                    "expected content hash '{expected_hash}', current is '{current_hash}'"
                )),
            ));
        }

        let mut warnings = Vec::new();
        if command.changes.is_empty() {
            warnings.push("no changes supplied; update was accepted as a no-op".to_string());
        }

        // Build domain event
        let event = ContextUpdatedEvent {
            root_node_id: case_id.as_str().to_string(),
            role: role.as_str().to_string(),
            revision: current_revision + 1,
            content_hash: content_hash.clone(),
            changes: command
                .changes
                .iter()
                .map(|c| ContextEventChange {
                    operation: c.operation.clone(),
                    entity_kind: c.entity_kind.clone(),
                    entity_id: c.entity_id.clone(),
                    payload_json: c.payload_json.clone(),
                    reason: if c.reason.is_empty() {
                        None
                    } else {
                        Some(c.reason.clone())
                    },
                    scopes: c.scopes.clone(),
                })
                .collect(),
            idempotency_key: command.idempotency_key.clone(),
            logical_digest: command.logical_digest.clone(),
            requested_by: command.requested_by.clone(),
            occurred_at: SystemTime::now(),
        };

        // Append with optimistic concurrency
        let new_revision = self.event_store.append(event, current_revision).await?;

        let projection_mutations =
            memory_projection_mutations(&command, new_revision, &content_hash)?;
        if !projection_mutations.is_empty() {
            self.projection_writer
                .apply_mutations(projection_mutations)
                .await?;
        }

        Ok(UpdateContextOutcome {
            accepted_version: AcceptedVersion {
                revision: new_revision,
                content_hash,
                generator_version: self.generator_version.to_string(),
            },
            warnings,
        })
    }
}

/// Deterministic SHA-256 hash of context changes for optimistic concurrency.
/// Stable across process restarts and machines.
fn compute_content_hash(changes: &[UpdateContextChange]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for change in changes {
        hasher.update(change.operation.as_bytes());
        hasher.update(change.entity_kind.as_bytes());
        hasher.update(change.entity_id.as_bytes());
        hasher.update(change.payload_json.as_bytes());
    }
    format!("{:064x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rehydration_domain::ProjectionMutation;
    use rehydration_testkit::{InMemoryContextEventStore, InMemoryProjectionWriter};

    use super::*;

    fn event_store() -> Arc<InMemoryContextEventStore> {
        Arc::new(InMemoryContextEventStore::new())
    }

    fn use_case(
        store: Arc<InMemoryContextEventStore>,
    ) -> UpdateContextUseCase<InMemoryContextEventStore> {
        UpdateContextUseCase::new(store, "0.1.0")
    }

    fn sample_change() -> UpdateContextChange {
        UpdateContextChange {
            operation: "UPDATE".to_string(),
            entity_kind: "node_detail".to_string(),
            entity_id: "node-1".to_string(),
            payload_json: r#"{"status":"ACTIVE"}"#.to_string(),
            reason: "test".to_string(),
            scopes: vec!["graph".to_string()],
        }
    }

    fn memory_changes() -> Vec<UpdateContextChange> {
        vec![
            UpdateContextChange {
                operation: "UPSERT".to_string(),
                entity_kind: "memory_dimension".to_string(),
                entity_id: "conversation:mcp".to_string(),
                payload_json: r#"{"id":"conversation:mcp","kind":"conversation","title":"MCP smoke"}"#.to_string(),
                reason: "test dimension".to_string(),
                scopes: vec!["memory".to_string()],
            },
            UpdateContextChange {
                operation: "UPSERT".to_string(),
                entity_kind: "memory_entry".to_string(),
                entity_id: "claim:mcp".to_string(),
                payload_json: r#"{"id":"claim:mcp","kind":"claim","text":"MCP ingest materializes into the read model.","coordinates":[{"dimension":"conversation","scope_id":"conversation:mcp","sequence":2,"occurred_at":"2026-05-04T10:00:00Z","valid_from":"2026-05-04T10:00:00Z"}]}"#.to_string(),
                reason: "test entry".to_string(),
                scopes: vec!["memory".to_string()],
            },
            UpdateContextChange {
                operation: "UPSERT".to_string(),
                entity_kind: "memory_relation".to_string(),
                entity_id: "relation:mcp".to_string(),
                payload_json: r#"{"from":"claim:old","to":"claim:mcp","rel":"supersedes","class":"evidential","why":"The new memory updates the previous claim.","confidence":"high"}"#.to_string(),
                reason: "test relation".to_string(),
                scopes: vec!["memory".to_string()],
            },
            UpdateContextChange {
                operation: "UPSERT".to_string(),
                entity_kind: "memory_evidence".to_string(),
                entity_id: "evidence:mcp".to_string(),
                payload_json: r#"{"id":"evidence:mcp","supports":["claim:mcp"],"text":"Projection writer recorded the detail.","source":"unit-test"}"#.to_string(),
                reason: "test evidence".to_string(),
                scopes: vec!["memory".to_string()],
            },
        ]
    }

    #[tokio::test]
    async fn execute_appends_event_and_returns_revision_one() {
        let store = event_store();
        let uc = use_case(Arc::clone(&store));

        let outcome = uc
            .execute(UpdateContextCommand {
                root_node_id: "node-1".to_string(),
                role: "developer".to_string(),
                work_item_id: "task-1".to_string(),
                changes: vec![sample_change()],
                expected_revision: None,
                expected_content_hash: None,
                idempotency_key: None,
                logical_digest: None,
                requested_by: None,
            })
            .await
            .expect("should succeed");

        assert_eq!(outcome.accepted_version.revision, 1);
        assert!(!outcome.accepted_version.content_hash.is_empty());
        assert!(outcome.warnings.is_empty());
    }

    #[tokio::test]
    async fn execute_rejects_wrong_expected_revision() {
        let store = event_store();
        let uc = use_case(Arc::clone(&store));

        let err = uc
            .execute(UpdateContextCommand {
                root_node_id: "node-1".to_string(),
                role: "developer".to_string(),
                work_item_id: "task-1".to_string(),
                changes: vec![sample_change()],
                expected_revision: Some(99),
                expected_content_hash: None,
                idempotency_key: None,
                logical_digest: None,
                requested_by: None,
            })
            .await;

        assert!(err.is_err());
        let msg = err.expect_err("should fail").to_string();
        assert!(msg.contains("expected revision 99"));
    }

    #[tokio::test]
    async fn execute_returns_cached_outcome_for_duplicate_idempotency_key() {
        let store = event_store();
        let uc = use_case(Arc::clone(&store));

        let first = uc
            .execute(UpdateContextCommand {
                root_node_id: "node-1".to_string(),
                role: "developer".to_string(),
                work_item_id: "task-1".to_string(),
                changes: vec![sample_change()],
                expected_revision: None,
                expected_content_hash: None,
                idempotency_key: Some("idem-1".to_string()),
                logical_digest: None,
                requested_by: None,
            })
            .await
            .expect("first call should succeed");

        let second = uc
            .execute(UpdateContextCommand {
                root_node_id: "node-1".to_string(),
                role: "developer".to_string(),
                work_item_id: "task-1".to_string(),
                changes: vec![sample_change()],
                expected_revision: None,
                expected_content_hash: None,
                idempotency_key: Some("idem-1".to_string()),
                logical_digest: None,
                requested_by: None,
            })
            .await
            .expect("second call should return cached outcome");

        assert_eq!(
            first.accepted_version.revision,
            second.accepted_version.revision
        );
        assert_eq!(
            first.accepted_version.content_hash,
            second.accepted_version.content_hash
        );
    }

    #[tokio::test]
    async fn execute_rejects_duplicate_idempotency_key_with_different_content() {
        let store = event_store();
        let uc = use_case(Arc::clone(&store));

        uc.execute(UpdateContextCommand {
            root_node_id: "node-1".to_string(),
            role: "developer".to_string(),
            work_item_id: "task-1".to_string(),
            changes: vec![sample_change()],
            expected_revision: None,
            expected_content_hash: None,
            idempotency_key: Some("idem-1".to_string()),
            logical_digest: None,
            requested_by: None,
        })
        .await
        .expect("first call should succeed");

        let mut changed = sample_change();
        changed.payload_json = r#"{"status":"CHANGED"}"#.to_string();
        let error = uc
            .execute(UpdateContextCommand {
                root_node_id: "node-1".to_string(),
                role: "developer".to_string(),
                work_item_id: "task-1".to_string(),
                changes: vec![changed],
                expected_revision: None,
                expected_content_hash: None,
                idempotency_key: Some("idem-1".to_string()),
                logical_digest: None,
                requested_by: None,
            })
            .await
            .expect_err("duplicate idempotency key with changed payload should fail");

        assert!(error.to_string().contains("different content"));
    }

    #[tokio::test]
    async fn execute_warns_on_empty_changes() {
        let store = event_store();
        let uc = use_case(Arc::clone(&store));

        let outcome = uc
            .execute(UpdateContextCommand {
                root_node_id: "node-1".to_string(),
                role: "developer".to_string(),
                work_item_id: "task-1".to_string(),
                changes: vec![],
                expected_revision: None,
                expected_content_hash: None,
                idempotency_key: None,
                logical_digest: None,
                requested_by: None,
            })
            .await
            .expect("empty changes should succeed with warning");

        assert_eq!(outcome.warnings.len(), 1);
        assert!(outcome.warnings[0].contains("no changes supplied"));
    }

    #[tokio::test]
    async fn execute_increments_revision_on_sequential_calls() {
        let store = event_store();
        let uc = use_case(Arc::clone(&store));

        let first = uc
            .execute(UpdateContextCommand {
                root_node_id: "node-1".to_string(),
                role: "developer".to_string(),
                work_item_id: "task-1".to_string(),
                changes: vec![sample_change()],
                expected_revision: None,
                expected_content_hash: None,
                idempotency_key: None,
                logical_digest: None,
                requested_by: None,
            })
            .await
            .expect("first should succeed");

        let second = uc
            .execute(UpdateContextCommand {
                root_node_id: "node-1".to_string(),
                role: "developer".to_string(),
                work_item_id: "task-2".to_string(),
                changes: vec![sample_change()],
                expected_revision: Some(1),
                expected_content_hash: None,
                idempotency_key: None,
                logical_digest: None,
                requested_by: None,
            })
            .await
            .expect("second should succeed");

        assert_eq!(first.accepted_version.revision, 1);
        assert_eq!(second.accepted_version.revision, 2);
    }

    #[tokio::test]
    async fn execute_rejects_wrong_content_hash_precondition() {
        let store = event_store();
        let uc = use_case(Arc::clone(&store));

        // First command establishes a hash
        let first = uc
            .execute(UpdateContextCommand {
                root_node_id: "node-1".to_string(),
                role: "developer".to_string(),
                work_item_id: "task-1".to_string(),
                changes: vec![sample_change()],
                expected_revision: None,
                expected_content_hash: None,
                idempotency_key: None,
                logical_digest: None,
                requested_by: None,
            })
            .await
            .expect("first should succeed");

        // Second command with wrong expected_content_hash must fail
        let err = uc
            .execute(UpdateContextCommand {
                root_node_id: "node-1".to_string(),
                role: "developer".to_string(),
                work_item_id: "task-2".to_string(),
                changes: vec![sample_change()],
                expected_revision: Some(1),
                expected_content_hash: Some("wrong-hash".to_string()),
                idempotency_key: None,
                logical_digest: None,
                requested_by: None,
            })
            .await;

        assert!(err.is_err());
        let msg = err.expect_err("should fail").to_string();
        assert!(msg.contains("expected content hash"));

        // Same command with correct hash succeeds
        let ok = uc
            .execute(UpdateContextCommand {
                root_node_id: "node-1".to_string(),
                role: "developer".to_string(),
                work_item_id: "task-2".to_string(),
                changes: vec![sample_change()],
                expected_revision: Some(1),
                expected_content_hash: Some(first.accepted_version.content_hash.clone()),
                idempotency_key: None,
                logical_digest: None,
                requested_by: None,
            })
            .await
            .expect("correct hash should succeed");

        assert_eq!(ok.accepted_version.revision, 2);
    }

    #[tokio::test]
    async fn execute_projects_memory_changes_into_read_model_mutations() {
        let store = event_store();
        let writer = InMemoryProjectionWriter::default();
        let uc = UpdateContextUseCase::new_with_projection_writer(
            Arc::clone(&store),
            writer.clone(),
            "0.1.0",
        );

        uc.execute(UpdateContextCommand {
            root_node_id: "question:mcp".to_string(),
            role: "memory".to_string(),
            work_item_id: "ingest:mcp:1".to_string(),
            changes: memory_changes(),
            expected_revision: None,
            expected_content_hash: None,
            idempotency_key: Some("ingest:mcp:1".to_string()),
            logical_digest: None,
            requested_by: Some("mcp-test".to_string()),
        })
        .await
        .expect("memory ingest should succeed");

        let mutations = writer.mutations().await;
        assert!(mutations.iter().any(|mutation| matches!(
            mutation,
            ProjectionMutation::UpsertNode(node)
                if node.node_id == "claim:mcp"
                    && node.node_kind == "claim"
                    && node.summary == "MCP ingest materializes into the read model."
        )));
        assert!(mutations.iter().any(|mutation| matches!(
            mutation,
            ProjectionMutation::UpsertNodeDetail(detail)
                if detail.node_id == "claim:mcp"
                    && detail.detail == "MCP ingest materializes into the read model."
        )));
        assert!(mutations.iter().any(|mutation| matches!(
            mutation,
            ProjectionMutation::UpsertNode(node)
                if node.node_id == "evidence:mcp"
                    && node.node_kind == "memory_evidence"
                    && node.summary == "Projection writer recorded the detail."
        )));
        assert!(mutations.iter().any(|mutation| matches!(
            mutation,
            ProjectionMutation::UpsertNodeRelation(relation)
                if relation.source_node_id == "question:mcp"
                    && relation.target_node_id == "claim:mcp"
                    && relation.relation_type == "records"
        )));
        assert!(mutations.iter().any(|mutation| matches!(
            mutation,
            ProjectionMutation::UpsertNodeRelation(relation)
                if relation.source_node_id == "conversation:mcp"
                    && relation.target_node_id == "claim:mcp"
                    && relation.relation_type == "contains_entry"
                    && relation.explanation.dimension() == Some("conversation")
                    && relation.explanation.scope_id() == Some("conversation:mcp")
                    && relation.explanation.sequence() == Some(2)
                    && relation.explanation.occurred_at() == Some("2026-05-04T10:00:00Z")
                    && relation.explanation.valid_from() == Some("2026-05-04T10:00:00Z")
        )));
        assert!(mutations.iter().any(|mutation| matches!(
            mutation,
            ProjectionMutation::UpsertNodeRelation(relation)
                if relation.source_node_id == "evidence:mcp"
                    && relation.target_node_id == "claim:mcp"
                    && relation.relation_type == "supports"
        )));
        assert!(mutations.iter().any(|mutation| matches!(
            mutation,
            ProjectionMutation::UpsertNodeRelation(relation)
                if relation.source_node_id == "claim:old"
                    && relation.target_node_id == "claim:mcp"
                    && relation.relation_type == "supersedes"
        )));
    }

    #[tokio::test]
    async fn execute_does_not_reproject_duplicate_idempotency_key() {
        let store = event_store();
        let writer = InMemoryProjectionWriter::default();
        let uc = UpdateContextUseCase::new_with_projection_writer(
            Arc::clone(&store),
            writer.clone(),
            "0.1.0",
        );

        for _ in 0..2 {
            uc.execute(UpdateContextCommand {
                root_node_id: "question:mcp".to_string(),
                role: "memory".to_string(),
                work_item_id: "ingest:mcp:1".to_string(),
                changes: memory_changes(),
                expected_revision: None,
                expected_content_hash: None,
                idempotency_key: Some("ingest:mcp:1".to_string()),
                logical_digest: None,
                requested_by: Some("mcp-test".to_string()),
            })
            .await
            .expect("idempotent memory ingest should succeed");
        }

        let mutations = writer.mutations().await;
        let claim_nodes = mutations
            .iter()
            .filter(|mutation| {
                matches!(
                    mutation,
                    ProjectionMutation::UpsertNode(node) if node.node_id == "claim:mcp"
                )
            })
            .count();
        assert_eq!(claim_nodes, 1);
    }
}
