//! The proposed search expansions of an Ingest (P15) and what became of
//! them, on the wire. They travel beside the entries and are never part of
//! the command that commits them.

use kmp_application::{SearchExpansionProposal, SearchExpansionReport};
use kmp_domain::SearchExpansions;
use kmp_proto::v1beta1::{
    IngestRequest, RefusedSearchExpansion, SearchExpansionsReport, StoredSearchExpansions,
};

use super::scalars::{ProtoMappingResult, invalid_argument};

/// Each entry's proposed expansions, in the writer's order. An entry may
/// propose at most [`SearchExpansions::MAX_EXPANSIONS`]; more is the
/// writer's error, as it is in `kmp_write_memory`. Entries that propose
/// nothing are left out, so an older client yields no proposal.
pub fn search_expansion_proposals_from_proto(
    request: &IngestRequest,
) -> ProtoMappingResult<Vec<SearchExpansionProposal>> {
    let Some(memory) = request.memory.as_ref() else {
        return Ok(Vec::new());
    };
    memory
        .entries
        .iter()
        .filter(|entry| !entry.search_expansions.is_empty())
        .map(|entry| {
            if entry.search_expansions.len() > SearchExpansions::MAX_EXPANSIONS {
                return Err(invalid_argument(format!(
                    "memory.entries[{}].search_expansions holds {} expansions; at most {} are kept per memory",
                    entry.id,
                    entry.search_expansions.len(),
                    SearchExpansions::MAX_EXPANSIONS
                )));
            }
            Ok(SearchExpansionProposal {
                entry_ref: entry.id.clone(),
                text: entry.text.clone(),
                expansions: entry.search_expansions.clone(),
            })
        })
        .collect()
}

pub fn search_expansions_report_to_proto(report: SearchExpansionReport) -> SearchExpansionsReport {
    SearchExpansionsReport {
        stored: report
            .stored
            .into_iter()
            .map(|(reference, expansions)| StoredSearchExpansions {
                r#ref: reference,
                expansions,
            })
            .collect(),
        refused: report
            .refused
            .into_iter()
            .map(|refused| RefusedSearchExpansion {
                r#ref: refused.entry_ref,
                expansion: refused.expansion,
                why: refused.why,
            })
            .collect(),
        not_stored: report.not_stored.unwrap_or_default(),
        judged_by: report.judged_by.unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use kmp_application::RefusedSearchExpansion as Refused;
    use kmp_proto::v1beta1::{Memory, MemoryEntry};

    use super::*;

    fn request(expansions: Vec<&str>) -> IngestRequest {
        IngestRequest {
            memory: Some(Memory {
                entries: vec![
                    MemoryEntry {
                        id: "question:wire:rollout".into(),
                        text: "The rollout slipped.".into(),
                        search_expansions: expansions.into_iter().map(str::to_string).collect(),
                        ..MemoryEntry::default()
                    },
                    MemoryEntry {
                        id: "question:wire:canteen".into(),
                        text: "The canteen menu changed.".into(),
                        ..MemoryEntry::default()
                    },
                ],
                ..Memory::default()
            }),
            ..IngestRequest::default()
        }
    }

    #[test]
    fn only_entries_that_propose_are_read_and_an_older_client_proposes_nothing() {
        assert!(
            search_expansion_proposals_from_proto(&request(vec![]))
                .expect("proposals")
                .is_empty()
        );
        assert!(
            search_expansion_proposals_from_proto(&IngestRequest::default())
                .expect("no memory")
                .is_empty()
        );
        assert_eq!(
            search_expansion_proposals_from_proto(&request(vec!["Why was the launch late?"]))
                .expect("proposals"),
            vec![SearchExpansionProposal {
                entry_ref: "question:wire:rollout".into(),
                text: "The rollout slipped.".into(),
                expansions: vec!["Why was the launch late?".into()],
            }]
        );
    }

    #[test]
    fn more_expansions_than_a_memory_keeps_is_an_invalid_argument() {
        let status =
            search_expansion_proposals_from_proto(&request(vec!["a"; 7])).expect_err("too many");
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
        assert!(
            status.message().contains("at most 6"),
            "{}",
            status.message()
        );
    }

    #[test]
    fn the_report_maps_field_for_field() {
        let report = SearchExpansionReport {
            stored: BTreeMap::from([("r1".to_string(), vec!["kept".to_string()])]),
            refused: vec![Refused {
                entry_ref: "r1".into(),
                expansion: "echo".into(),
                why: "repeats".into(),
            }],
            not_stored: None,
            judged_by: Some("jev noul>=0.50".into()),
        };
        assert_eq!(
            search_expansions_report_to_proto(report),
            SearchExpansionsReport {
                stored: vec![StoredSearchExpansions {
                    r#ref: "r1".into(),
                    expansions: vec!["kept".into()],
                }],
                refused: vec![RefusedSearchExpansion {
                    r#ref: "r1".into(),
                    expansion: "echo".into(),
                    why: "repeats".into(),
                }],
                not_stored: String::new(),
                judged_by: "jev noul>=0.50".into(),
            }
        );
    }
}
