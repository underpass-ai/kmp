use super::{ProjectedClaim, ViewRevision};
use kmp_domain::PortError;
use kmp_domain::consolidation::{
    ConsolidationRead, ConsolidationReadStatus, ConsolidationSelection,
};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ProjectedConsolidation {
    pub status: ConsolidationReadStatus,
    pub about: Option<String>,
    pub view: Option<String>,
    pub revision: Option<u64>,
    pub claims: Vec<ProjectedClaim>,
    pub omitted_claims: usize,
    pub changed_sources: Vec<String>,
    pub selection: Option<ConsolidationSelection>,
    /// Exact read request for the immutable audit, not a time-filtered proof.
    pub expansion: Option<ViewRevision>,
    pub semantic_validation: &'static str,
}

pub fn project(
    read: &ConsolidationRead,
    selection: Option<ConsolidationSelection>,
    max_bytes: usize,
) -> Result<ProjectedConsolidation, PortError> {
    if !(512..=1_048_576).contains(&max_bytes) {
        return Err(PortError::InvalidState(
            "projection max_bytes must be 512..1048576".into(),
        ));
    }
    if let Some(selection) = &selection {
        selection.validate()?;
    }
    let mut result = ProjectedConsolidation {
        status: read.status,
        about: None,
        view: None,
        revision: None,
        claims: vec![],
        omitted_claims: 0,
        changed_sources: read.changed_sources.clone(),
        selection: selection.clone(),
        expansion: None,
        semantic_validation: "writer-declared; quote and dependency validation does not establish meaning or independent corroboration",
    };
    if let Some(view) = &read.view {
        let eligible = selection
            .as_ref()
            .map(|s| s.eligible_sources(view))
            .transpose()?;
        result.about = Some(view.about.clone());
        result.view = Some(view.view.clone());
        result.revision = Some(view.revision);
        result.expansion = Some(ViewRevision {
            about: view.about.clone(),
            view: view.view.clone(),
            revision: view.revision,
        });
        result.omitted_claims = view.claims.len();
        for claim in &view.claims {
            if eligible
                .as_ref()
                .is_some_and(|refs| refs.len() != view.sources.len())
            {
                continue;
            }
            let source_refs = claim
                .supports
                .iter()
                .map(|a| a.source_ref.clone())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            let supported: Vec<_> = view
                .sources
                .iter()
                .filter(|source| {
                    claim
                        .supports
                        .iter()
                        .any(|a| a.source_ref == source.reference)
                })
                .collect();
            let source_states = supported
                .iter()
                .map(|s| (s.reference.clone(), s.status.clone()))
                .collect();
            let mut lifecycle_relations: Vec<_> = supported
                .iter()
                .flat_map(|s| s.relations.iter())
                .filter(|(key, _)| {
                    key.get(2).is_some_and(|rel| {
                        matches!(rel.as_str(), "contradicts" | "corrects" | "supersedes")
                    })
                })
                .cloned()
                .collect();
            lifecycle_relations.sort();
            lifecycle_relations.dedup();
            result.claims.push(ProjectedClaim {
                identity: claim.identity.clone(),
                source_refs,
                source_states,
                lifecycle_relations,
            });
            result.omitted_claims -= 1;
            if serde_json::to_vec(&result)
                .map_err(|e| PortError::InvalidState(e.to_string()))?
                .len()
                > max_bytes
            {
                result.claims.pop();
                result.omitted_claims += 1;
            }
        }
    }
    if serde_json::to_vec(&result)
        .map_err(|e| PortError::InvalidState(e.to_string()))?
        .len()
        > max_bytes
    {
        return Err(PortError::InvalidState(
            "projection metadata exceeds max_bytes; request a larger budget".into(),
        ));
    }
    Ok(result)
}
