use kmp_embedded::EmbeddedMemoryService;
use kmp_proto_mapping::v1beta1::{condense_command_from_proto, condense_response_from_card};
use serde_json::Value;

use super::embedded_errors::{kernel_error, mapping_error};
use crate::projection::condense_from_response;
use crate::serving::adapters::grpc::requests::condense_request_from_arguments;
use crate::serving::{ToolError, tool_success_result};

pub(super) async fn embedded_condense(
    service: &EmbeddedMemoryService,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request =
        condense_request_from_arguments(arguments).map_err(ToolError::invalid_argument)?;
    let about = request.about.clone();
    let command = condense_command_from_proto(request, kernel_now_rfc3339())
        .map_err(|status| mapping_error(&status))?;
    let outcome = service
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

/// The kernel's own clock, formatted the way stored instants are.
fn kernel_now_rfc3339() -> String {
    kmp_domain::rfc3339_from_epoch_seconds(crate::clock::now_seconds())
}
