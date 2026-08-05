use std::sync::Arc;

use rehydration_application::{
    CommandApplicationService, NoopProjectionWriter, UpdateContextChange, UpdateContextCommand,
};
use rehydration_domain::{ContextEventStore, ProjectionWriter};
use rehydration_proto::v1beta1::{
    CommandMetadata, RevisionPrecondition, UpdateContextRequest, UpdateContextResponse,
    context_command_service_server::ContextCommandService,
};
use tonic::{Request, Response, Status};

use crate::transport::proto_mapping_v1beta1::proto_accepted_version_v1beta1;
use crate::transport::support::map_application_error;

#[derive(Debug, Clone)]
pub struct CommandGrpcServiceV1Beta1<E, W = NoopProjectionWriter> {
    application: Arc<CommandApplicationService<E, W>>,
}

impl<E, W> CommandGrpcServiceV1Beta1<E, W> {
    pub fn new(application: Arc<CommandApplicationService<E, W>>) -> Self {
        Self { application }
    }
}

#[tonic::async_trait]
impl<E, W> ContextCommandService for CommandGrpcServiceV1Beta1<E, W>
where
    E: ContextEventStore + Send + Sync + 'static,
    W: ProjectionWriter + Send + Sync + 'static,
{
    #[tracing::instrument(skip(self, request), fields(rpc = "UpdateContext"))]
    async fn update_context(
        &self,
        request: Request<UpdateContextRequest>,
    ) -> Result<Response<UpdateContextResponse>, Status> {
        let request = request.into_inner();
        tracing::debug!(
            root_node_id = %request.root_node_id,
            role = %request.role,
            changes = request.changes.len(),
            "handling update_context"
        );
        let metadata = request.metadata.unwrap_or(CommandMetadata {
            idempotency_key: String::new(),
            correlation_id: String::new(),
            causation_id: String::new(),
            requested_by: String::new(),
            requested_at: None,
        });
        let precondition = request.precondition.unwrap_or(RevisionPrecondition {
            expected_revision: 0,
            expected_content_hash: String::new(),
        });

        let outcome = self
            .application
            .update_context(UpdateContextCommand {
                root_node_id: request.root_node_id,
                role: request.role,
                work_item_id: request.work_item_id,
                changes: request
                    .changes
                    .into_iter()
                    .map(|change| UpdateContextChange {
                        operation: change.operation().as_str_name().to_string(),
                        entity_kind: change.entity_kind,
                        entity_id: change.entity_id,
                        payload_json: change.payload_json,
                        reason: change.reason,
                        scopes: change.scopes,
                    })
                    .collect(),
                logical_digest: None,
                expected_revision: (precondition.expected_revision != 0)
                    .then_some(precondition.expected_revision),
                expected_content_hash: (!precondition.expected_content_hash.is_empty())
                    .then_some(precondition.expected_content_hash),
                idempotency_key: (!metadata.idempotency_key.is_empty())
                    .then_some(metadata.idempotency_key),
                requested_by: (!metadata.requested_by.is_empty()).then_some(metadata.requested_by),
            })
            .await
            .map_err(map_application_error)?;

        Ok(Response::new(UpdateContextResponse {
            accepted_version: Some(proto_accepted_version_v1beta1(&outcome.accepted_version)),
            warnings: outcome.warnings,
        }))
    }
}
