use serde_json::{Value, json};

use kmp_proto::v1beta1::TraceResponse;

use super::rendering::*;

pub(crate) fn trace_from_response(response: TraceResponse) -> Value {
    json!({
        "summary": response.summary,
        "trace": response.trace.iter().map(memory_relation_json).collect::<Vec<_>>(),
        "page": response
            .page
            .as_ref()
            .map(page_info_json)
            .unwrap_or_else(empty_page_info_json),
        "quality": optional_quality_json(response.quality.as_ref()),
        "warnings": response.warnings
    })
}
