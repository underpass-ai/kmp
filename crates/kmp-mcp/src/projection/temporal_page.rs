//! A temporal selection is stable; its entries and proof can cross response pages.
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use super::serialized_size::serialized_len;
use super::temporal_entry_projection::TemporalEntryProjection;
use crate::serving::ToolError;
use kmp_proto_mapping::v1beta1::recall_projection::requested_byte_limit;

const CURSOR_VERSION: &str = "kmpt1";
const SECTIONS: &[&str] = &[
    "/entries",
    "/proof/entries",
    "/proof/evidence",
    "/proof/path",
    "/proof/conflicts",
    "/proof/expired",
    "/proof/superseded",
    "/proof/missing",
    "/proof/matched_relations",
    "/proof/matched_terms",
    "/proof/groups",
    "/raw_refs",
];

/// One bounded kernel selection, replayed under the same arguments and content.
/// No mutable reader session or store scan is introduced by response pagination.
#[derive(Clone)]
pub(crate) struct TemporalPage {
    core: Value,
    items: Vec<(&'static str, Value)>,
    arguments: Value,
    tool: String,
    hash: String,
    navigation: Vec<Value>,
}

impl TemporalPage {
    pub(crate) fn project(value: Value, arguments: &Value) -> Result<Value, ToolError> {
        let limit = requested_byte_limit(arguments).map_err(ToolError::invalid_argument)?;
        let plan = Self::new(value, arguments)?;
        let offset = plan.offset()?;
        let available = plan.items.len().saturating_sub(offset);
        let cap = match arguments.pointer("/page/entries") {
            None => available,
            Some(value) => value
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .filter(|n| *n > 0)
                .ok_or_else(|| {
                    ToolError::invalid_argument("page.entries must be a positive integer")
                })?
                .min(available),
        };
        let complete = plan.render(offset, cap);
        if serialized_len(&complete) <= limit {
            return Ok(complete);
        }
        // The final page has fewer continuation fields; test it above, then
        // search only partial pages whose size grows with their item prefix.
        let (mut low, mut high) = (0, cap.saturating_sub(1));
        while low < high {
            let middle = low + (high - low).div_ceil(2);
            if serialized_len(&plan.render(offset, middle)) <= limit {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        let mut page = plan.render(offset, low);
        if low == 0 && available > 0 {
            // An indivisible source or core can still exceed the requested
            // budget. Return a concrete retry that admits at least one item,
            // never an unchanged zero-progress cursor loop.
            let required = plan.minimum_progress_bytes(offset);
            let mut retry = plan.page_action(offset);
            retry["arguments"]["budget"]["max_bytes"] = json!(required);
            page["next_actions"] = json!([retry]);
            page["page"]["minimum_progress_bytes"] = json!(required);
            page["warnings"].as_array_mut().expect("warnings").push(json!(format!(
                "budget.max_bytes {limit} cannot fit the next complete item; execute next_actions to retry with {required} bytes"
            )));
        } else if serialized_len(&page) > limit {
            page["warnings"]
                .as_array_mut()
                .expect("warnings")
                .push(json!(format!(
                    "budget.max_bytes {limit} is below the stable selection metadata floor"
                )));
        }
        Ok(page)
    }

    fn new(mut value: Value, arguments: &Value) -> Result<Self, ToolError> {
        let direction = value
            .pointer("/temporal/direction")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::backend("temporal response lacks a direction"))?;
        let tool = match direction {
            "goto" | "near" | "forward" | "rewind" => format!("kmp_{direction}"),
            _ => {
                return Err(ToolError::backend(
                    "temporal response has an invalid direction",
                ));
            }
        };
        let fields = TemporalEntryProjection::read(arguments)?;
        let navigation = navigation(&value, arguments);
        let kernel_page = value["page"].clone();
        value
            .as_object_mut()
            .expect("temporal object")
            .remove("next_action");
        value
            .as_object_mut()
            .expect("temporal object")
            .remove("page");
        value["selection"] = json!({
            "scope":"selected_packet",
            "entries":value["entries"].as_array().map_or(0, Vec::len),
            "matching_entries":kernel_page["total"],
            "has_more":kernel_page["has_more"]
        });
        let mut bound = arguments.clone();
        let object = bound
            .as_object_mut()
            .ok_or_else(|| ToolError::invalid_argument("arguments must be an object"))?;
        object.remove("page");
        if let Some(budget) = object.get_mut("budget").and_then(Value::as_object_mut) {
            budget.remove("max_bytes");
            if budget.is_empty() {
                object.remove("budget");
            }
        }
        let mut hash = Sha256::new();
        hash.update(CURSOR_VERSION);
        hash.update(serde_json::to_vec(&bound).expect("arguments serialize"));
        hash.update(serde_json::to_vec(&value).expect("temporal response serializes"));
        // Bind full content before projection: a hidden-field change still
        // invalidates a cursor instead of mixing different source versions.
        if let Some(fields) = fields {
            fields.apply(&mut value, arguments)?;
        }
        let mut items = Vec::new();
        for section in SECTIONS {
            if let Some(values) = value.pointer_mut(section).and_then(Value::as_array_mut) {
                items.extend(
                    std::mem::take(values)
                        .into_iter()
                        .map(|value| (*section, value)),
                );
            }
        }
        Ok(Self {
            core: value,
            items,
            arguments: arguments.clone(),
            tool,
            hash: format!("{:x}", hash.finalize()),
            navigation,
        })
    }

    fn offset(&self) -> Result<usize, ToolError> {
        let Some(cursor) = self.arguments.pointer("/page/cursor") else {
            return Ok(0);
        };
        let mut parts = cursor.as_str().unwrap_or_default().split(':');
        let version = parts.next();
        let offset = parts.next().and_then(|n| n.parse::<usize>().ok());
        let hash = parts.next();
        if version != Some(CURSOR_VERSION)
            || offset.is_none()
            || hash.is_none()
            || parts.next().is_some()
        {
            return Err(ToolError::invalid_argument(
                "malformed temporal page.cursor",
            ));
        }
        if hash != Some(self.hash.as_str()) {
            let mut restart = self.arguments.clone();
            restart.as_object_mut().expect("arguments").remove("page");
            return Err(ToolError::conflict(
                "temporal cursor selection or content changed; restart the read",
            )
            .with_feedback(
                json!({"code":"READ_SELECTION_CHANGED","field":"page.cursor",
                    "action":{"tool":self.tool,"arguments":restart}}),
            ));
        }
        let offset = offset.expect("checked");
        if offset >= self.items.len() {
            return Err(ToolError::invalid_argument(
                "temporal page.cursor is exhausted or out of range",
            ));
        }
        Ok(offset)
    }

    fn minimum_progress_bytes(&self, offset: usize) -> usize {
        let mut retry = self.clone();
        let mut required = serialized_len(&retry.render(offset, 1));
        loop {
            retry.arguments["budget"]["max_bytes"] = json!(required);
            let measured = serialized_len(&retry.render(offset, 1));
            if measured <= required {
                return required;
            }
            required = measured;
        }
    }

    fn page_action(&self, offset: usize) -> Value {
        let mut arguments = self.arguments.clone();
        arguments["page"]["cursor"] = json!(format!("{CURSOR_VERSION}:{offset}:{}", self.hash));
        json!({"tool":self.tool,"arguments":arguments})
    }

    fn render(&self, offset: usize, keep: usize) -> Value {
        let end = (offset + keep).min(self.items.len());
        let mut result = self.core.clone();
        for (section, value) in &self.items[offset..end] {
            result
                .pointer_mut(section)
                .and_then(Value::as_array_mut)
                .expect("temporal section")
                .push(value.clone());
        }
        let has_more = end < self.items.len();
        let mut sections = Map::new();
        for section in SECTIONS {
            if self.core.pointer(section).is_none() {
                continue;
            }
            let count =
                |slice: &[(&str, Value)]| slice.iter().filter(|(key, _)| key == section).count();
            sections.insert(
                section.trim_start_matches('/').replace('/', "."),
                json!({
                    "returned_on_page":count(&self.items[offset..end]),
                    "remaining":count(&self.items[end..]), "total":count(&self.items),
                }),
            );
        }
        result["page"] = json!({"offset":offset,"returned":end-offset,"total":self.items.len(),
            "has_more":has_more,"next_cursor":has_more.then(||format!("{CURSOR_VERSION}:{end}:{}",self.hash)),
            "sections":sections});
        result["next_actions"] = if has_more {
            json!([self.page_action(end)])
        } else {
            json!(self.navigation)
        };
        result
    }
}

fn navigation(value: &Value, arguments: &Value) -> Vec<Value> {
    if value.pointer("/page/has_more").and_then(Value::as_bool) != Some(true) {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut add = |tool: &str, reference: Option<&str>| {
        let Some(reference) = reference.filter(|r| !r.is_empty()) else {
            return;
        };
        let mut args = arguments.clone();
        let object = args.as_object_mut().expect("arguments");
        for key in ["at", "around", "from", "page", "window"] {
            object.remove(key);
        }
        args["from"] = json!({"ref":reference});
        result.push(json!({"tool":tool,"arguments":args}));
    };
    let boundary = value.pointer("/page/next_cursor").and_then(Value::as_str);
    match value.pointer("/temporal/direction").and_then(Value::as_str) {
        Some("goto" | "rewind") => add("kmp_rewind", boundary),
        Some("forward") => add("kmp_forward", boundary),
        Some("near") => {
            let entries = value["entries"].as_array();
            add(
                "kmp_rewind",
                entries
                    .and_then(|e| e.first())
                    .and_then(|e| e["ref"].as_str()),
            );
            add(
                "kmp_forward",
                entries
                    .and_then(|e| e.last())
                    .and_then(|e| e["ref"].as_str()),
            );
        }
        _ => {}
    }
    result
}
