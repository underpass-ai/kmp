use kmp_domain::{NodeCardExpectation, TraceCondenseCandidates};
use kmp_proto::v1beta1::{self as proto, trace_condense_candidate::Expect};

pub(super) fn project(candidates: TraceCondenseCandidates) -> proto::TraceCondenseCandidates {
    proto::TraceCondenseCandidates {
        omitted_count: candidates.omitted_count,
        below_floor: candidates.below_floor,
        valid: candidates.valid,
        after_cut: candidates.after_cut,
        items: candidates
            .items
            .into_iter()
            .map(|candidate| {
                let descriptor = candidate.descriptor;
                proto::TraceCondenseCandidate {
                    r#ref: descriptor.node_id,
                    body_bytes: descriptor.body_bytes,
                    record_bytes: descriptor.record_bytes,
                    card_status: candidate.card_status.as_str().into(),
                    shared_by: candidate.shared_by,
                    source_revision: descriptor.revision,
                    source_record_digest: descriptor.record_digest,
                    expect: Some(match candidate.expect {
                        NodeCardExpectation::Absent => Expect::Absent(true),
                        NodeCardExpectation::CardRevision(revision) => {
                            Expect::CardRevision(revision)
                        }
                    }),
                }
            })
            .collect(),
    }
}
