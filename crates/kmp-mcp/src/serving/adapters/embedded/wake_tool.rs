use super::super::embedded_errors::{kernel_error, mapping_error};
use super::read_telemetry::EmbeddedReadTelemetry;
use crate::projection::wake_from_response;
use crate::serving::adapters::tool_request_mapping::WakeRequestMapper;
use crate::serving::{ToolError, tool_success_result};
use kmp_embedded::EmbeddedMemoryService;
use kmp_proto_mapping::v1beta1::recall_projection::project_wake_response;
use kmp_proto_mapping::v1beta1::{wake_query_from_proto, wake_response_with_focus, wake_sources};
use serde_json::Value;
use std::sync::Arc;

use super::super::wake_focus_judge::WakeFocusJudge;

/// Maps one wake read and projects its observed result.
pub(crate) struct EmbeddedWakeTool<'a> {
    service: &'a EmbeddedMemoryService,
    telemetry: EmbeddedReadTelemetry<'a>,
    focus: &'a Result<Option<Arc<WakeFocusJudge>>, String>,
}

impl<'a> EmbeddedWakeTool<'a> {
    pub(crate) fn new(
        service: &'a EmbeddedMemoryService,
        telemetry: EmbeddedReadTelemetry<'a>,
        focus: &'a Result<Option<Arc<WakeFocusJudge>>, String>,
    ) -> Self {
        Self {
            service,
            telemetry,
            focus,
        }
    }

    pub(crate) async fn call(&self, arguments: &Value) -> Result<Value, ToolError> {
        let request =
            WakeRequestMapper::from_arguments(arguments).map_err(ToolError::invalid_argument)?;
        let query =
            wake_query_from_proto(request.clone()).map_err(|status| mapping_error(&status))?;
        let intent = query.intent.clone();
        let max_entries = query.max_entries;
        let temporal = query.temporal.clone();
        let about = query.about.clone();
        let result = self
            .service
            .wake(query)
            .await
            .map_err(kernel_error("wake", &about))?;
        self.telemetry
            .trace_timing("kmp_wake", result.timing.as_ref());
        self.telemetry
            .observe("kmp_wake", &result.bundle, &result.rendered.quality);
        // Focus needs a stated intent: with none, there is nothing to judge
        // relevance against, and the wake is the ordinary one.
        let mut warnings = Vec::new();
        let mut selection = None;
        match self.focus {
            Ok(Some(judge)) if !intent.trim().is_empty() => {
                let sources =
                    wake_sources(&result, &temporal).map_err(|status| mapping_error(&status))?;
                let continuation = arguments
                    .get("page")
                    .and_then(|page| page.get("cursor"))
                    .is_some();
                let outcome = judge
                    .focus(&intent, &sources, continuation)
                    .await
                    .map_err(ToolError::invalid_argument)?;
                selection = outcome.selection;
                warnings.extend(outcome.warning);
            }
            Err(error) => warnings.push(format!("wake focus disabled: {error}")),
            _ => {}
        }
        let mut focused =
            wake_response_with_focus(&intent, max_entries, result, &temporal, selection.as_ref())
                .map_err(|status| mapping_error(&status))?;
        focused.warnings.extend(warnings);
        let response = project_wake_response(focused, &request)
            .map_err(crate::projection::recall_error::projection)?;
        Ok(tool_success_result(wake_from_response(response)))
    }
}
