use super::super::embedded_errors::{kernel_error, mapping_error};
use super::commit_native_write_guard::CommitNativeWriteGuard;
use crate::projection::ingest_from_response;
use crate::serving::adapters::tool_request_mapping::IngestRequestMapper;
use crate::serving::{ToolError, tool_success_result};
use kmp_embedded::EmbeddedMemoryService;
use kmp_embedded::{CommitNativeBundle, EmbeddedKernelStore};
use kmp_proto_mapping::v1beta1::{ingest_command_from_proto, ingest_response_from_outcome};
use serde_json::Value;

/// Maps and executes one ingest, guarding canonical bundle publication when configured.
pub(crate) struct EmbeddedIngestTool<'a> {
    service: &'a EmbeddedMemoryService,
    commit_native: Option<&'a CommitNativeBundle>,
    store: &'a EmbeddedKernelStore,
}

impl<'a> EmbeddedIngestTool<'a> {
    pub(crate) fn new(
        service: &'a EmbeddedMemoryService,
        commit_native: Option<&'a CommitNativeBundle>,
        store: &'a EmbeddedKernelStore,
    ) -> Self {
        Self {
            service,
            commit_native,
            store,
        }
    }

    pub(crate) async fn call(&self, arguments: &Value) -> Result<Value, ToolError> {
        let writes = !arguments
            .get("dry_run")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let Some(bundle) = self.commit_native.filter(|_| writes) else {
            return self.ingest(arguments).await;
        };
        // Begin before parsing: the guard's preflight error takes precedence.
        let guard = CommitNativeWriteGuard::begin(bundle, self.store).await?;
        let result = self.ingest(arguments).await;
        guard.finish(result).await
    }

    async fn ingest(&self, arguments: &Value) -> Result<Value, ToolError> {
        let request =
            IngestRequestMapper::from_arguments(arguments).map_err(ToolError::invalid_argument)?;
        let command =
            ingest_command_from_proto(request).map_err(|status| mapping_error(&status))?;
        let about = command.about.clone();
        let outcome = self
            .service
            .ingest(command)
            .await
            .map_err(kernel_error("ingest", &about))?;
        Ok(tool_success_result(ingest_from_response(
            ingest_response_from_outcome(outcome),
        )))
    }
}
