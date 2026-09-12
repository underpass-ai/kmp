//! Which canonical bodies one bounded response may carry, decided before any
//! of them is read.
//!
//! The input is the selected manifest and one descriptor per ref. Nothing here
//! touches body text, and nothing here can change the manifest: admission is
//! downstream of discovery, ranking and selection, and reducing nodes, edges,
//! depth or paths is never how a body budget is met.

use std::collections::{BTreeMap, BTreeSet};

use crate::{NodeBodyDescriptor, TraceBodyOptions, TraceBodyState};

/// The decision, per ref, plus what it cost and what the next step would cost.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TraceBodyAdmission {
    /// Refs whose canonical body must be read, in manifest order. Only these
    /// reach the body port.
    pub load: Vec<String>,
    pub states: BTreeMap<String, TraceBodyState>,
    /// Stored record bytes this response admitted. What the ceiling spent.
    pub admitted_record_bytes: u64,
    /// Descriptor total over every present body in the manifest, loaded or
    /// not. Separate from `body_bytes`, which counts what was actually read.
    pub selected_body_bytes: u64,
    /// The first record the ceiling refused, if any: its ref and its exact
    /// stored size.
    pub next_deferred: Option<(String, u64)>,
}

impl TraceBodyAdmission {
    /// Smallest ceiling that would admit one more record if the whole query
    /// were rerun from the start. The already admitted prefix is paid again,
    /// so this is not the size of the deferred record.
    pub fn next_rerun_record_bytes(&self) -> Option<u64> {
        self.next_deferred
            .as_ref()
            .map(|(_, bytes)| self.admitted_record_bytes + bytes)
    }

    /// Smallest ceiling that would admit one more record as a named expansion.
    /// A named batch pays for nothing it did not name, so this is exactly the
    /// record's own size.
    pub fn next_named_record_bytes(&self) -> Option<u64> {
        self.next_deferred.as_ref().map(|(_, bytes)| *bytes)
    }

    pub fn state(&self, node_id: &str) -> TraceBodyState {
        self.states
            .get(node_id)
            .copied()
            .unwrap_or(TraceBodyState::Missing)
    }

    /// Refs whose body the store holds and this response withheld.
    pub fn omitted(&self) -> BTreeSet<String> {
        self.states
            .iter()
            .filter(|(_, state)| state.is_omitted())
            .map(|(id, _)| id.clone())
            .collect()
    }
}

/// Decides delivery for every ref of `manifest`, which must be sorted and
/// deduplicated.
///
/// `reusable` names the refs a valid card already stands for. Three rules, in
/// this order, and no implicit fourth:
///
/// 1. A named expansion wins. A ref the caller asked to expand loads its
///    canonical body under the ceiling and is never answered with a card —
///    an expansion that returns the card again is not an expansion.
/// 2. A compact read loads nothing. Where a valid card stands for the body it
///    says so; where the card is stale, absent, post-cut or in another
///    language it says *that*, with the descriptor and the exact cost of
///    expanding, and the body stays unread. Falling back to the canonical
///    record exactly when a card stops being usable would load the large
///    shared source at the worst possible moment, which is the cost this
///    whole path exists to avoid.
/// 3. Otherwise the ceiling admits a deterministic sorted prefix.
///
/// `descriptors` missing a ref means the store holds no body for it. An
/// inconsistent projection is the port's error and never reaches here.
pub fn admit(
    manifest: &[String],
    descriptors: &BTreeMap<String, NodeBodyDescriptor>,
    options: &TraceBodyOptions,
    reusable: &BTreeSet<String>,
) -> TraceBodyAdmission {
    let mut result = TraceBodyAdmission::default();
    // Once one present record does not fit, everything after it defers too.
    // Skipping ahead to whatever happens to be small would make the split
    // depend on sizes rather than on order, and would let a large record
    // disappear from a response that looks complete.
    let mut ceiling_reached = false;
    for node_id in manifest {
        let Some(descriptor) = descriptors.get(node_id) else {
            result
                .states
                .insert(node_id.clone(), TraceBodyState::Missing);
            continue;
        };
        result.selected_body_bytes += descriptor.body_bytes;
        let named = options.refs.as_ref().map(|refs| refs.contains(node_id));
        match named {
            Some(false) => {
                result
                    .states
                    .insert(node_id.clone(), state_without_body(node_id, options, reusable));
                continue;
            }
            // No named expansion, and a compact read asks for cards, not for
            // canonical text.
            None if options.compact.is_some() => {
                result
                    .states
                    .insert(node_id.clone(), state_without_body(node_id, options, reusable));
                continue;
            }
            _ => {}
        }
        match options.max_record_bytes {
            Some(ceiling)
                if ceiling_reached
                    || result.admitted_record_bytes + descriptor.record_bytes > ceiling =>
            {
                ceiling_reached = true;
                result
                    .states
                    .insert(node_id.clone(), TraceBodyState::DeferredBudget);
                if result.next_deferred.is_none() {
                    result.next_deferred = Some((node_id.clone(), descriptor.record_bytes));
                }
            }
            _ => {
                result.admitted_record_bytes += descriptor.record_bytes;
                result.states.insert(node_id.clone(), TraceBodyState::Loaded);
                result.load.push(node_id.clone());
            }
        }
    }
    result
}

/// How a body this response does not load is reported: `Compact` when a valid
/// card stands for it, `NotRequested` otherwise. Never `Missing`, which is
/// reserved for a body the store does not hold.
fn state_without_body(
    node_id: &str,
    options: &TraceBodyOptions,
    reusable: &BTreeSet<String>,
) -> TraceBodyState {
    if options.compact.is_some() && reusable.contains(node_id) {
        TraceBodyState::Compact
    } else {
        TraceBodyState::NotRequested
    }
}

#[cfg(test)]
#[path = "trace_body_admission_tests.rs"]
mod tests;
