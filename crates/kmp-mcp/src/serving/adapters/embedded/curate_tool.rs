use serde_json::{Map, Value};

use super::super::curate_review_cache::CurateReviewCache;
use super::super::embedded_errors::{kernel_error, mapping_error};
use super::read_telemetry::EmbeddedReadTelemetry;
use crate::curate::application::dto::curate_review_dto::review_to_value;
use crate::curate::application::mappers::relate_material_mapper::relate_material;
use crate::curate::application::use_cases::review_relations::ReviewRelations;
use crate::serving::adapters::tool_request_mapping::RelateRequestMapper;
use crate::serving::ports::judgement_model::JudgementModel;
use crate::serving::{ToolError, tool_success_result};
use kmp_embedded::EmbeddedMemoryService;
use kmp_proto_mapping::v1beta1::{
    LexicalBridge, curate_reading_from_result, relate_query_from_proto,
};

const DEFAULT_MAX_PAIRS: u64 = 12;
const MAX_PAIRS: u64 = 40;
const DEFAULT_ENTRIES: u64 = 8;
const MAX_ENTRIES: u64 = 20;

/// Review mode of `kmp_curate` on the embedded store. A fresh review reads,
/// judges and freezes; a page with `review_token` reads the frozen review
/// and calls nothing.
pub(crate) struct EmbeddedCurateTool<'a> {
    service: &'a EmbeddedMemoryService,
    telemetry: EmbeddedReadTelemetry<'a>,
    bridge: &'a LexicalBridge,
    judgement: Option<&'a dyn JudgementModel>,
    judgement_warning: Option<&'a str>,
    cache: &'a CurateReviewCache,
}

impl<'a> EmbeddedCurateTool<'a> {
    pub(crate) fn new(
        service: &'a EmbeddedMemoryService,
        telemetry: EmbeddedReadTelemetry<'a>,
        bridge: &'a LexicalBridge,
        judgement: Option<&'a dyn JudgementModel>,
        judgement_warning: Option<&'a str>,
        cache: &'a CurateReviewCache,
    ) -> Self {
        Self {
            service,
            telemetry,
            bridge,
            judgement,
            judgement_warning,
            cache,
        }
    }

    pub(crate) async fn call(&self, arguments: &Value) -> Result<Value, ToolError> {
        if arguments.get("mode").and_then(Value::as_str) != Some("review") {
            return Err(ToolError::invalid_argument(
                "kmp_curate mode must be review",
            ));
        }
        let about = arguments
            .get("about")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::invalid_argument("kmp_curate requires about"))?
            .to_string();
        let page = arguments.get("page");
        let entries = page
            .and_then(|page| page.get("entries"))
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_ENTRIES)
            .clamp(1, MAX_ENTRIES) as usize;
        let offset = match page.and_then(|page| page.get("cursor")) {
            None | Some(Value::Null) => 0,
            Some(cursor) => cursor
                .as_str()
                .and_then(|cursor| cursor.parse::<usize>().ok())
                .ok_or_else(|| {
                    ToolError::invalid_argument(
                        "kmp_curate page.cursor is not a cursor it returned",
                    )
                })?,
        };
        if let Some(token) = arguments.get("review_token").and_then(Value::as_str) {
            let (review, material) = self
                .cache
                .get(token)
                .ok_or_else(|| ToolError::invalid_argument("review expired; run a fresh review"))?;
            return Ok(tool_success_result(review_to_value(
                &review, &material, &about, token, offset, entries,
            )));
        }
        let max_pairs = arguments
            .get("max_pairs")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_MAX_PAIRS)
            .min(MAX_PAIRS) as usize;
        let relate_arguments = Value::Object(
            arguments
                .as_object()
                .map(|object| {
                    object
                        .iter()
                        .filter(|(key, _)| {
                            !matches!(
                                key.as_str(),
                                "mode" | "max_pairs" | "review_token" | "page" | "context_id"
                            )
                        })
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect::<Map<_, _>>()
                })
                .unwrap_or_default(),
        );
        let request = RelateRequestMapper::from_arguments(&relate_arguments)
            .map_err(ToolError::invalid_argument)?;
        let query = relate_query_from_proto(request).map_err(|status| mapping_error(&status))?;
        let result = self
            .service
            .relate(query.clone())
            .await
            .map_err(kernel_error("curate", &about))?;
        self.telemetry
            .observe("kmp_curate", &result.bundle, &result.rendered.quality);
        let response = curate_reading_from_result(result, &query, self.bridge)
            .map_err(|status| mapping_error(&status))?;
        let material = relate_material(&response);
        let mut review = ReviewRelations {
            judgement: self.judgement,
        }
        .run(material.clone(), max_pairs)
        .await;
        if let Some(warning) = self.judgement_warning {
            review
                .warnings
                .insert(0, format!("Jev disabled: {warning}"));
        }
        let token = review.token();
        self.cache
            .insert(token.clone(), review.clone(), material.clone());
        Ok(tool_success_result(review_to_value(
            &review, &material, &about, &token, 0, entries,
        )))
    }
}
