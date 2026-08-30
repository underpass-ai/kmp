use serde_json::{Map, Value};

use std::collections::{BTreeMap, BTreeSet};

use super::arguments::{optional_array, required_map_string};

/// What the writer proved it read first, mapped from `read_context` into
/// the refs each recall tool vouches for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ReadContext {
    ref_sources: BTreeMap<String, BTreeSet<String>>,
}

impl ReadContext {
    pub(crate) fn from_arguments(arguments: &Map<String, Value>) -> Result<Self, String> {
        let Some(read_context) = arguments.get("read_context") else {
            return Ok(Self::default());
        };
        let read_context = read_context
            .as_object()
            .ok_or_else(|| "argument `read_context` must be an object".to_string())?;
        let mut context = Self::default();
        context.collect_ref_array(read_context, "inspected_refs", "kmp_inspect")?;
        context.collect_ref_array(read_context, "temporal_refs", "kmp_temporal")?;
        context.collect_ref_array(read_context, "wake_refs", "kmp_wake")?;
        context.collect_ref_array(read_context, "ask_refs", "kmp_ask")?;
        context.collect_trace_paths(read_context)?;
        Ok(context)
    }

    pub(crate) fn add_ref(&mut self, ref_id: &str, source: &str) {
        if ref_id.trim().is_empty() {
            return;
        }
        self.ref_sources
            .entry(ref_id.to_string())
            .or_default()
            .insert(source.to_string());
    }

    pub(crate) fn sources_for(&self, ref_id: &str) -> Vec<String> {
        self.ref_sources
            .get(ref_id)
            .map(|sources| sources.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub(crate) fn collect_ref_array(
        &mut self,
        object: &Map<String, Value>,
        key: &str,
        source: &str,
    ) -> Result<(), String> {
        for (index, ref_value) in optional_array(object.get(key), &format!("read_context.{key}"))?
            .iter()
            .enumerate()
        {
            let ref_id = ref_value
                .as_str()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| format!("read_context.{key}[{index}] must be a non-empty string"))?;
            self.add_ref(ref_id, source);
        }
        Ok(())
    }

    pub(crate) fn collect_trace_paths(
        &mut self,
        object: &Map<String, Value>,
    ) -> Result<(), String> {
        for (index, trace) in optional_array(object.get("trace_paths"), "read_context.trace_paths")?
            .iter()
            .enumerate()
        {
            let trace = trace
                .as_object()
                .ok_or_else(|| format!("read_context.trace_paths[{index}] must be an object"))?;
            let from = required_map_string(
                trace,
                "from",
                &format!("read_context.trace_paths[{index}].from"),
            )?;
            let to = required_map_string(
                trace,
                "to",
                &format!("read_context.trace_paths[{index}].to"),
            )?;
            self.add_ref(from, "kmp_trace");
            self.add_ref(to, "kmp_trace");
            for (ref_index, ref_value) in optional_array(
                trace.get("refs"),
                &format!("read_context.trace_paths[{index}].refs"),
            )?
            .iter()
            .enumerate()
            {
                let ref_id = ref_value
                    .as_str()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| {
                        format!(
                            "read_context.trace_paths[{index}].refs[{ref_index}] must be a non-empty string"
                        )
                    })?;
                self.add_ref(ref_id, "kmp_trace");
            }
        }
        Ok(())
    }
}
