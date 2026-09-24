use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use super::super::curate_doubt_cache::CurateDoubtCache;
use super::super::curate_review_cache::CurateReviewCache;
use super::super::embedded_errors::{kernel_error, mapping_error};
use super::read_telemetry::EmbeddedReadTelemetry;
use crate::curate::application::dto::curate_review_dto::review_to_value;
use crate::curate::application::dto::prepared_apply_dto::prepared_to_value;
use crate::curate::application::mappers::relate_material_mapper::relate_material;
use crate::curate::application::use_cases::prepare_apply::PrepareApply;
use crate::curate::application::use_cases::review_relations::ReviewRelations;
use crate::curate::domain::apply_item::ApplyItem;
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
    doubts: &'a CurateDoubtCache,
}

impl<'a> EmbeddedCurateTool<'a> {
    pub(crate) fn new(
        service: &'a EmbeddedMemoryService,
        telemetry: EmbeddedReadTelemetry<'a>,
        bridge: &'a LexicalBridge,
        judgement: Option<&'a dyn JudgementModel>,
        judgement_warning: Option<&'a str>,
        cache: &'a CurateReviewCache,
        doubts: &'a CurateDoubtCache,
    ) -> Self {
        Self {
            service,
            telemetry,
            bridge,
            judgement,
            judgement_warning,
            cache,
            doubts,
        }
    }

    pub(crate) async fn call(&self, arguments: &Value) -> Result<Value, ToolError> {
        match arguments.get("mode").and_then(Value::as_str) {
            Some("review") => {}
            // Internal: the apply dispatcher asks the store that froze the
            // review to resolve and pre-check what the agent accepted.
            Some("prepare_apply") => return self.prepare_apply(arguments).await,
            _ => {
                return Err(ToolError::invalid_argument(
                    "kmp_curate mode must be review or apply",
                ));
            }
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

    async fn prepare_apply(&self, arguments: &Value) -> Result<Value, ToolError> {
        let about = arguments
            .get("about")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::invalid_argument("kmp_curate apply requires about"))?;
        let token = arguments
            .get("review_token")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::invalid_argument("kmp_curate apply requires review_token"))?;
        let (review, material) = self
            .cache
            .get(token)
            .ok_or_else(|| ToolError::invalid_argument("review expired; run a fresh review"))?;
        let accepted = arguments
            .get("accepted")
            .and_then(Value::as_array)
            .filter(|items| !items.is_empty())
            .ok_or_else(|| {
                ToolError::invalid_argument("kmp_curate apply requires accepted items")
            })?;
        let items = accepted
            .iter()
            .map(apply_item)
            .collect::<Result<Vec<_>, _>>()?;
        let digest = format!(
            "{:x}",
            Sha256::digest(
                format!(
                    "kmp.curate.precheck.v1\0{about}\0{token}\0{}",
                    // Confirmation changes nothing Jev reads, so it cannot
                    // change the key: confirming reuses the frozen doubts.
                    Value::Array(
                        accepted
                            .iter()
                            .map(|item| {
                                let mut item = item.clone();
                                if let Some(object) = item.as_object_mut() {
                                    object.remove("confirm_doubted");
                                }
                                item
                            })
                            .collect()
                    )
                )
                .as_bytes()
            )
        );
        let frozen = self.doubts.get(&digest);
        let (prepared, doubts) = PrepareApply {
            judgement: self.judgement,
        }
        .run(about, &review, &material, items, frozen)
        .await;
        self.doubts.insert(digest, doubts);
        Ok(tool_success_result(prepared_to_value(&prepared)))
    }
}

fn apply_item(value: &Value) -> Result<ApplyItem, ToolError> {
    let text = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    let flag = |key: &str| value.get(key).and_then(Value::as_bool).unwrap_or(false);
    Ok(ApplyItem {
        item_id: text("item_id")
            .ok_or_else(|| ToolError::invalid_argument("accepted[].item_id is required"))?,
        why: text("why").ok_or_else(|| {
            ToolError::invalid_argument("accepted[].why is required: the reason is yours to write")
        })?,
        evidence: text("evidence").ok_or_else(|| {
            ToolError::invalid_argument("accepted[].evidence is required: say what shows it")
        })?,
        confidence: text("confidence"),
        rel: text("rel"),
        reverse: flag("reverse"),
        confirm_doubted: flag("confirm_doubted"),
    })
}
