use serde_json::{Map, Value};

use std::collections::{BTreeMap, BTreeSet};

use super::arguments::{optional_array, required_map_string};
use super::relate_proposal::RelateProposal;

/// What the writer proved it read first, mapped from `read_context` into
/// the refs each recall tool vouches for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ReadContext {
    ref_sources: BTreeMap<String, BTreeSet<String>>,
    relate_proposals: Vec<RelateProposal>,
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
        context.collect_relate_proposals(read_context)?;
        Ok(context)
    }

    /// The proposal that names `foreign_ref` beside a ref of `about`, when
    /// the writer handed one back: what makes an equivalence across abouts
    /// a declaration rather than a guess.
    pub(crate) fn relate_proposal_for(
        &self,
        about: &str,
        foreign_ref: &str,
    ) -> Option<&RelateProposal> {
        self.relate_proposals.iter().find(|proposal| {
            let other = if proposal.to == foreign_ref {
                Some(proposal.from.as_str())
            } else if proposal.from == foreign_ref {
                Some(proposal.to.as_str())
            } else {
                None
            };
            other.is_some_and(|other| {
                kmp_application::validate_supplied_member_ref(about, "read_context", other).is_ok()
            })
        })
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

    /// `read_context.relate_proposals`: each entry names `from`, `to` and
    /// the `proposed_by` signals exactly as `kmp_relate` returned them.
    pub(crate) fn collect_relate_proposals(
        &mut self,
        object: &Map<String, Value>,
    ) -> Result<(), String> {
        for (index, proposal) in optional_array(
            object.get("relate_proposals"),
            "read_context.relate_proposals",
        )?
        .iter()
        .enumerate()
        {
            let proposal = proposal.as_object().ok_or_else(|| {
                format!("read_context.relate_proposals[{index}] must be an object")
            })?;
            let from = required_map_string(
                proposal,
                "from",
                &format!("read_context.relate_proposals[{index}].from"),
            )?;
            let to = required_map_string(
                proposal,
                "to",
                &format!("read_context.relate_proposals[{index}].to"),
            )?;
            let mut proposed_by = Vec::new();
            for (signal_index, signal) in optional_array(
                proposal.get("proposed_by"),
                &format!("read_context.relate_proposals[{index}].proposed_by"),
            )?
            .iter()
            .enumerate()
            {
                let signal = signal
                    .as_str()
                    .filter(|value| matches!(*value, "identifier" | "summary" | "entity"))
                    .ok_or_else(|| {
                        format!(
                            "read_context.relate_proposals[{index}].proposed_by[{signal_index}] must be `identifier`, `summary` or `entity`"
                        )
                    })?;
                proposed_by.push(signal.to_string());
            }
            if proposed_by.is_empty() {
                return Err(format!(
                    "read_context.relate_proposals[{index}].proposed_by must name at least one signal kmp_relate returned"
                ));
            }
            self.add_ref(from, "kmp_relate");
            self.add_ref(to, "kmp_relate");
            self.relate_proposals.push(RelateProposal {
                from: from.to_string(),
                to: to.to_string(),
                proposed_by,
            });
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
