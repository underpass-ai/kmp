use super::super::embedded_errors::{kernel_error, mapping_error};
use crate::projection::condense_from_response;
use crate::serving::adapters::tool_request_mapping::CondenseRequestMapper;
use crate::serving::{ToolError, tool_success_result};
use kmp_embedded::EmbeddedMemoryService;
use kmp_proto_mapping::v1beta1::{condense_command_from_proto, condense_response_from_card};
use serde_json::Value;

/// Authors one reader card through the application; card policy stays in the kernel.
///
/// The kernel stamps `authored_at`: a caller-supplied instant could place a
/// card before a historical cut it never existed at. A refusal by the card
/// policy is a typed tool error, distinct from an application/store failure.
pub(crate) struct EmbeddedCondenseTool<'a> {
    service: &'a EmbeddedMemoryService,
}

impl<'a> EmbeddedCondenseTool<'a> {
    pub(crate) fn new(service: &'a EmbeddedMemoryService) -> Self {
        Self { service }
    }

    pub(crate) async fn call(&self, arguments: &Value) -> Result<Value, ToolError> {
        let request = CondenseRequestMapper::from_arguments(arguments)
            .map_err(ToolError::invalid_argument)?;
        let about = request.about.clone();
        let command = condense_command_from_proto(request, Self::kernel_now_rfc3339())
            .map_err(|status| mapping_error(&status))?;
        let outcome = self
            .service
            .condense(command)
            .await
            .map_err(kernel_error("condense", &about))?;
        match outcome {
            Ok(card) => Ok(tool_success_result(condense_from_response(
                condense_response_from_card(card),
            ))),
            Err(rejection) => {
                let message = rejection.to_string();
                Err(if rejection.is_conflict() {
                    ToolError::conflict(message)
                } else if rejection.is_not_found() {
                    ToolError::not_found(message)
                } else {
                    ToolError::invalid_argument(message)
                })
            }
        }
    }

    /// The kernel stamps authorship; caller time must not precede a historical cut.
    fn kernel_now_rfc3339() -> String {
        kmp_domain::rfc3339_from_epoch_seconds(crate::clock::now_seconds())
    }
}
