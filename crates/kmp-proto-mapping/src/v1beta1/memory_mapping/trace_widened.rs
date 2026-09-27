use kmp_domain::TraceSearchLimits;
use kmp_proto::v1beta1::{TraceRequest, TraceSearchOptions};

use super::scalars::{ProtoMappingResult, invalid_argument};

/// A single-destination trace with its own allowance: one `to`,
/// `search.direction: "bidirectional"` and nothing but work limits. It is
/// the `search.widen` a partial single-destination trace offers (DESIGN L7),
/// the same bounded bidirectional search over every stored relation, in
/// both directions and without filtering by why or evidence, under a larger
/// allowance. It is not the bounded target search, which walks one
/// direction of source-backed links.
pub(super) struct WidenedTrace;

impl WidenedTrace {
    pub(super) const DIRECTION: &'static str = "bidirectional";

    /// Whether `request` asks for the bidirectional single-destination trace.
    pub(super) fn requested(request: &TraceRequest) -> bool {
        request
            .search
            .as_ref()
            .is_some_and(|search| search.direction == Self::DIRECTION)
    }

    /// The allowance a bidirectional request brings, a zero limit taking the
    /// single-destination default; `None` when it asks for something else.
    pub(super) fn limits(request: &TraceRequest) -> ProtoMappingResult<Option<TraceSearchLimits>> {
        let Some(options) = request.search.as_ref().filter(|_| Self::requested(request)) else {
            return Ok(None);
        };
        if !request.targets.is_empty()
            || request.to.trim().is_empty()
            || request.as_of.is_some()
            || request.interval.is_some()
            || request.axis != 0
            || *options != Self::bare(options)
        {
            return Err(invalid_argument(
                "search.direction bidirectional is a single-destination trace: one `to`, no \
                 time selection, and only max_nodes, max_edges, max_depth and max_states",
            ));
        }
        let default = TraceSearchLimits::single_destination();
        let or = |value: u32, fallback: u32| if value == 0 { fallback } else { value };
        let limits = TraceSearchLimits {
            nodes: or(options.max_nodes, default.nodes),
            edges: or(options.max_edges, default.edges),
            depth: or(options.max_depth, default.depth),
            states: or(options.max_states, default.states),
        };
        limits
            .validate()
            .map_err(|error| invalid_argument(error.to_string()))?;
        Ok(Some(limits))
    }

    /// `options` with every field but the work limits and the direction
    /// cleared (one path per target is the only one it returns anyway).
    fn bare(options: &TraceSearchOptions) -> TraceSearchOptions {
        TraceSearchOptions {
            max_nodes: options.max_nodes,
            max_edges: options.max_edges,
            max_depth: options.max_depth,
            max_states: options.max_states,
            direction: options.direction.clone(),
            paths_per_target: options.paths_per_target.min(1),
            ..TraceSearchOptions::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(search: TraceSearchOptions) -> TraceRequest {
        TraceRequest {
            about: "p".into(),
            from: "a".into(),
            to: "b".into(),
            search: Some(search),
            ..TraceRequest::default()
        }
    }

    fn bidirectional() -> TraceSearchOptions {
        TraceSearchOptions {
            direction: WidenedTrace::DIRECTION.into(),
            ..TraceSearchOptions::default()
        }
    }

    #[test]
    fn a_bidirectional_request_brings_its_allowance_or_the_single_destination_default() {
        assert_eq!(
            WidenedTrace::limits(&request(bidirectional())).expect("valid"),
            Some(TraceSearchLimits::single_destination())
        );
        let widest = TraceSearchLimits::widest();
        let wide = request(TraceSearchOptions {
            max_nodes: widest.nodes,
            max_edges: widest.edges,
            max_depth: widest.depth,
            max_states: widest.states,
            ..bidirectional()
        });
        assert_eq!(WidenedTrace::limits(&wide).expect("valid"), Some(widest));
    }

    #[test]
    fn a_bidirectional_request_is_a_trace_query_not_a_bounded_search() {
        let widened = request(TraceSearchOptions {
            max_nodes: 4096,
            ..bidirectional()
        });
        assert!(
            super::super::trace_search::trace_search_request_from_proto(&widened)
                .expect("valid")
                .is_none()
        );
        let query = super::super::queries::trace_query_from_proto(widened).expect("valid");
        assert_eq!(query.limits.map(|limits| limits.nodes), Some(4096));
        let plain = super::super::queries::trace_query_from_proto(TraceRequest {
            to: "b".into(),
            ..TraceRequest::default()
        })
        .expect("valid");
        assert_eq!(plain.limits, None);
    }

    #[test]
    fn other_directions_are_not_this_trace() {
        let outgoing = request(TraceSearchOptions {
            direction: "outgoing".into(),
            ..TraceSearchOptions::default()
        });
        assert!(!WidenedTrace::requested(&outgoing));
        assert_eq!(WidenedTrace::limits(&outgoing).expect("not ours"), None);
        let plain = TraceRequest {
            to: "b".into(),
            ..TraceRequest::default()
        };
        assert_eq!(WidenedTrace::limits(&plain).expect("not ours"), None);
    }

    #[test]
    fn anything_but_work_limits_or_a_single_to_is_refused() {
        let filtered = request(TraceSearchOptions {
            relations: vec!["supports".into()],
            ..bidirectional()
        });
        assert!(WidenedTrace::limits(&filtered).is_err());
        let proof = request(TraceSearchOptions {
            proof: true,
            ..bidirectional()
        });
        assert!(WidenedTrace::limits(&proof).is_err());
        let mut several = request(bidirectional());
        several.targets = vec!["b".into(), "c".into()];
        assert!(WidenedTrace::limits(&several).is_err());
        let too_wide = request(TraceSearchOptions {
            max_nodes: TraceSearchLimits::widest().nodes + 1,
            ..bidirectional()
        });
        assert!(WidenedTrace::limits(&too_wide).is_err());
        let one_path = request(TraceSearchOptions {
            paths_per_target: 1,
            ..bidirectional()
        });
        assert!(WidenedTrace::limits(&one_path).is_ok());
    }
}
