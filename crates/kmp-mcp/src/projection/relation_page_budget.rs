//! Byte-bounded prefixes of the Trace/Relate page already selected by the kernel.
//! Items remain whole. The cursor advances only over items actually returned;
//! summaries and Relate's selection proof form the stable response floor.
use std::collections::BTreeSet;

use kmp_proto_mapping::v1beta1::recall_projection::requested_byte_limit;
use serde_json::{Value, json};

use super::relation_cursor::RelationCursor;
use super::serialized_size::serialized_len;
use crate::serving::ToolError;

pub(crate) enum RelationPageBudget {
    Trace,
    Relate,
}

impl RelationPageBudget {
    fn sections(&self) -> &'static [&'static str] {
        match self {
            Self::Trace => &["trace"],
            Self::Relate => &["facts", "declared", "coordinate", "tensions", "proposed"],
        }
    }

    pub(crate) fn apply(
        &self,
        mut value: Value,
        arguments: &Value,
        fingerprint: &str,
    ) -> Result<Value, ToolError> {
        if matches!(self, Self::Trace) {
            super::trace_material_expansion::attach(&mut value, arguments);
        }
        let limit = requested_byte_limit(arguments).map_err(ToolError::invalid_argument)?;
        let total = value["page"]["total"].as_u64().unwrap_or(0) as usize;
        let tool = match self {
            Self::Trace => "kmp_trace",
            Self::Relate => "kmp_relate",
        };
        let cursor = RelationCursor::read(tool, fingerprint, arguments, total)?;
        let available = self
            .sections()
            .iter()
            .filter_map(|section| value[*section].as_array())
            .map(Vec::len)
            .sum::<usize>();
        let render = |count| self.render(&value, &cursor, count);
        let complete = render(available);
        if serialized_len(&complete) <= limit {
            return Ok(complete);
        }
        let mut best = render(0);
        let (mut low, mut high) = (1, available);
        while low <= high {
            let middle = low + (high - low) / 2;
            let candidate = render(middle);
            if serialized_len(&candidate) <= limit {
                best = candidate;
                low = middle + 1;
            } else {
                high = middle - 1;
            }
        }
        if best["page"]["returned"] == 0 && available > 0 {
            // Retrying with this allowance fits at least the next indivisible
            // item, even when the remainder of the backend page is much larger.
            let mut required = serialized_len(&render(1));
            loop {
                let measured =
                    serialized_len(&self.render(&value, &cursor.with_budget(required), 1));
                if measured <= required {
                    break;
                }
                required = measured;
            }
            best["page"]["required_bytes"] = json!(required);
            best["next_actions"] = json!([cursor.with_budget(required).action(cursor.offset)]);
            best["warnings"].as_array_mut().expect("Trace/Relate mapper emits array sections").push(json!(
                "budget.max_bytes cannot fit the next whole item; repeat page.next_cursor with budget.max_bytes at least page.required_bytes"
            ));
        }
        if serialized_len(&best) > limit {
            // Include the warning itself in the named floor. Iterate only over
            // the decimal digit count, never over the source text or evidence.
            let warnings = best["warnings"]
                .as_array_mut()
                .expect("Trace/Relate mapper emits array sections");
            warnings.push(json!(""));
            let index = warnings.len() - 1;
            let mut size = 0;
            loop {
                best["warnings"][index] = json!(format!(
                    "budget.max_bytes {limit} is below this response's stable floor; returned the {size}-byte floor instead"
                ));
                let measured = serialized_len(&best);
                if measured == size {
                    break;
                }
                size = measured;
            }
        }
        Ok(best)
    }

    fn render(&self, original: &Value, cursor: &RelationCursor, count: usize) -> Value {
        let offset = cursor.offset;
        let mut value = original.clone();
        value["warnings"]
            .as_array_mut()
            .expect("warnings")
            .retain(|warning| {
                !matches!(
                    warning.as_str(),
                    Some(
                        "trace response paginated; use page.next_cursor to continue"
                            | "relate response paginated; use page.next_cursor to continue"
                    )
                )
            });
        let mut remaining = count;
        for section in self.sections() {
            let items = value[*section]
                .as_array_mut()
                .expect("Trace/Relate mapper emits array sections");
            let retained = remaining.min(items.len());
            remaining -= retained;
            items.truncate(retained);
        }
        let total = value["page"]["total"].as_u64().unwrap_or(0) as usize;
        let end = offset.min(total).saturating_add(count).min(total);
        let has_more = end < total;
        value["page"] = json!({"offset":offset,"returned":count,"total":total,"has_more":has_more,
            "next_cursor":has_more.then(|| cursor.token(end))});
        value["next_actions"] = if has_more {
            json!([cursor.action(end)])
        } else {
            json!([])
        };
        if has_more {
            value["warnings"]
                .as_array_mut()
                .expect("Trace/Relate mapper emits array sections")
                .push(json!(
                    "response is partial; execute next_actions to continue the same selection"
                ));
        }
        if matches!(self, Self::Trace) && value["quality"].is_object() {
            let edges = value["trace"]
                .as_array()
                .expect("Trace mapper emits a relation array");
            let nodes: BTreeSet<_> = edges
                .iter()
                .flat_map(|edge| ["from", "to"].map(|key| &edge[key]))
                .filter_map(Value::as_str)
                .filter(|reference| !reference.is_empty())
                .collect();
            let causal = edges
                .iter()
                .filter(|edge| edge["class"] == "causal")
                .count();
            value["quality"] = json!({"nodes":nodes.len(),"relationships":edges.len(),
                "details":0,"detail_coverage":0.0,
                "causal_density":if edges.is_empty() {0.0} else {causal as f64 / edges.len() as f64},
                "truncated":has_more});
        }
        value
    }
}
