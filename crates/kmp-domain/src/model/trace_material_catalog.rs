use super::trace_material_set::TraceMaterialSet;
use crate::{DomainError, TraceProofRequirement, TraceSearchResult};
use std::collections::{BTreeMap, BTreeSet};

/// Bounded native path material and explicit requirements in one identity universe.
pub(super) struct TraceMaterialCatalog {
    pub names: Vec<String>,
    pub paths: Vec<TraceMaterialSet>,
    pub groups: Vec<(u32, Vec<TraceMaterialSet>)>,
}

impl TraceMaterialCatalog {
    pub fn new(
        result: &TraceSearchResult,
        groups: &[TraceProofRequirement],
    ) -> Result<Self, DomainError> {
        let mut names = BTreeSet::from([result.from.clone()]);
        let mut routes = Vec::new();
        for route in &result.routes {
            let mut current = result.from.as_str();
            let mut nodes = BTreeSet::from([current.to_string()]);
            for &index in &route.edge_indexes {
                let edge = result.relations.get(index as usize).ok_or_else(|| {
                    DomainError::InvalidState("trace route contains an absent edge index".into())
                })?;
                current = if edge.source_node_id == current {
                    &edge.target_node_id
                } else if edge.target_node_id == current {
                    &edge.source_node_id
                } else {
                    return Err(DomainError::InvalidState(
                        "trace route has disconnected edges".into(),
                    ));
                };
                if !nodes.insert(current.to_string()) {
                    return Err(DomainError::InvalidState(
                        "trace material expects simple paths".into(),
                    ));
                }
            }
            if current != route.target {
                return Err(DomainError::InvalidState(
                    "trace route does not reach its target".into(),
                ));
            }
            names.extend(nodes.iter().cloned());
            routes.push(nodes);
        }
        // Unknown/unreached requirements remain in the universe, not vacuously covered.
        for group in groups {
            for alternative in &group.alternatives {
                names.extend(alternative.iter().cloned());
            }
        }
        let names = names.into_iter().collect::<Vec<_>>();
        let positions = names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.as_str(), i))
            .collect::<BTreeMap<_, _>>();
        let mask = |refs: &BTreeSet<String>| {
            let mut set = TraceMaterialSet::empty(names.len());
            for name in refs {
                set.insert(positions[name.as_str()]);
            }
            set
        };
        let paths = routes.iter().map(mask).collect();
        let groups = groups
            .iter()
            .map(|g| (g.weight, g.alternatives.iter().map(mask).collect()))
            .collect();
        Ok(Self {
            names,
            paths,
            groups,
        })
    }

    /// Returns true coverage and exact 1680*(F + H/2), never a probability.
    pub fn score(&self, material: &TraceMaterialSet) -> (u32, u64, Vec<u32>) {
        let mut benefit = 0;
        let mut partial = 0;
        let mut covered = Vec::new();
        for (index, (weight, alternatives)) in self.groups.iter().enumerate() {
            if alternatives
                .iter()
                .any(|a| material.intersection_count(a) == a.count())
            {
                benefit += weight;
                covered.push(index as u32);
            } else {
                partial += u64::from(*weight)
                    * alternatives
                        .iter()
                        .map(|a| {
                            u64::from(material.intersection_count(a)) * (840 / u64::from(a.count()))
                        })
                        .max()
                        .unwrap_or(0);
            }
        }
        (benefit, 1680 * u64::from(benefit) + partial, covered)
    }
}
