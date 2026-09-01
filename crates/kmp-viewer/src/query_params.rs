//! Query-parameter decoding for the memory read routes: each helper turns
//! one raw parameter into the kernel's vocabulary, or into a refusal that
//! names what was wrong with it — extracted from the route handlers as the
//! 2026-08-28 conformance audit prescribed.

use kmp_domain::{
    DimensionSelection, ResolutionTier, TemporalAxis, TemporalCursor, TemporalDirection,
};

use crate::http::{HttpRequest, HttpResponse};

const DEFAULT_GRAPH_DEPTH: u32 = 2;
const MAX_GRAPH_DEPTH: u32 = 6;
const DEFAULT_TOKEN_BUDGET: u32 = 16_384;
const MAX_TOKEN_BUDGET: u32 = 262_144;
const DEFAULT_WINDOW_ENTRIES: usize = 8;
const MAX_WINDOW_ENTRIES: usize = 256;

/// A number a caller sent, or a refusal naming what was wrong with it.
///
/// Absent means "use the default"; present and unparseable means the caller
/// believes they asked for something. Answering 200 to `depth=abc` as though
/// it read `depth=2` is the one thing every other refusal in this codebase is
/// written not to do — `scope` and `dims` next door already say so by name.
/// Out of range is still clamped: a bound is a policy, not a mistake.
pub(crate) fn numeric_param<T>(
    request: &HttpRequest,
    key: &str,
    default: T,
) -> Result<T, HttpResponse>
where
    T: std::str::FromStr,
{
    match request.param(key) {
        None => Ok(default),
        Some(value) => value.parse::<T>().map_err(|_| {
            HttpResponse::error(
                400,
                &format!("parameter `{key}` is not a number: `{value}`"),
            )
        }),
    }
}

pub(crate) fn depth_param(request: &HttpRequest) -> Result<u32, HttpResponse> {
    Ok(numeric_param(request, "depth", DEFAULT_GRAPH_DEPTH)?.clamp(1, MAX_GRAPH_DEPTH))
}

pub(crate) fn budget_param(request: &HttpRequest) -> Result<u32, HttpResponse> {
    Ok(numeric_param(request, "budget", DEFAULT_TOKEN_BUDGET)?.clamp(256, MAX_TOKEN_BUDGET))
}

pub(crate) fn window_param(request: &HttpRequest, key: &str) -> Result<usize, HttpResponse> {
    Ok(numeric_param(request, key, DEFAULT_WINDOW_ENTRIES)?.min(MAX_WINDOW_ENTRIES))
}

/// `scope=all` widens recall to every about the kernel indexes — the global
/// graph. `dims=a,b` restricts to those dimension kinds. Defaults mirror
/// `kmp_wake`: the current about, all dimensions.
pub(crate) fn dimension_selection(
    request: &HttpRequest,
) -> Result<DimensionSelection, HttpResponse> {
    let selection = match request.param("dims") {
        Some(dims) => {
            let kinds: Vec<String> = dims
                .split(',')
                .map(str::trim)
                .filter(|kind| !kind.is_empty())
                .map(ToString::to_string)
                .collect();
            if kinds.is_empty() {
                return Err(HttpResponse::error(400, "parameter `dims` holds no kinds"));
            }
            DimensionSelection::only(kinds)
        }
        None => DimensionSelection::all(),
    };
    Ok(match request.param("scope") {
        Some("all") => selection.with_all_about_scope(),
        Some("current") | None => selection,
        Some(other) => {
            return Err(HttpResponse::error(
                400,
                &format!("unknown scope `{other}`; expected `current` or `all`"),
            ));
        }
    })
}

pub(crate) fn tier_param(request: &HttpRequest) -> Result<Option<ResolutionTier>, HttpResponse> {
    match request.param("tier") {
        None => Ok(None),
        Some("summary") => Ok(Some(ResolutionTier::L0Summary)),
        Some("spine") => Ok(Some(ResolutionTier::L1CausalSpine)),
        Some("evidence") => Ok(Some(ResolutionTier::L2EvidencePack)),
        Some(other) => Err(HttpResponse::error(
            400,
            &format!("unknown tier `{other}`; expected `summary`, `spine` or `evidence`"),
        )),
    }
}

/// Exactly one of `ref`, `time`, `seq` — the same contract the MCP temporal
/// tools enforce.
pub(crate) fn cursor_param(request: &HttpRequest) -> Result<TemporalCursor, HttpResponse> {
    let cursor = match (
        request.param("ref"),
        request.param("time"),
        request.param("seq"),
    ) {
        (Some(ref_id), None, None) => TemporalCursor::ref_id(ref_id),
        (None, Some(time), None) => TemporalCursor::time(time),
        (None, None, Some(seq)) => match seq.parse::<u32>() {
            Ok(seq) => TemporalCursor::sequence(seq),
            Err(_) => {
                return Err(HttpResponse::error(
                    400,
                    "parameter `seq` must be a positive integer",
                ));
            }
        },
        _ => {
            return Err(HttpResponse::error(
                400,
                "the temporal cursor requires exactly one of `ref`, `time`, or `seq`",
            ));
        }
    };
    cursor.map_err(|error| HttpResponse::error(400, &error.to_string()))
}

pub(crate) fn direction_param(request: &HttpRequest) -> Result<TemporalDirection, HttpResponse> {
    match request.param("direction") {
        None | Some("near") => Ok(TemporalDirection::Near),
        Some("goto") => Ok(TemporalDirection::Goto),
        Some("rewind") => Ok(TemporalDirection::Rewind),
        Some("forward") => Ok(TemporalDirection::Forward),
        Some(other) => Err(HttpResponse::error(
            400,
            &format!("unknown direction `{other}`; expected `goto`, `near`, `rewind` or `forward`"),
        )),
    }
}

pub(crate) fn axis_param(request: &HttpRequest) -> Result<TemporalAxis, HttpResponse> {
    match request.param("axis").or_else(|| request.param("clock")) {
        None => Ok(TemporalAxis::Default),
        Some("occurred") => Ok(TemporalAxis::Occurred),
        Some("observed") => Ok(TemporalAxis::Observed),
        Some("ingested") => Ok(TemporalAxis::Ingested),
        Some("validity") => Ok(TemporalAxis::Validity),
        Some(other) => Err(HttpResponse::error(
            400,
            &format!(
                "unknown temporal axis `{other}`; expected `occurred`, `observed`, `ingested` or `validity`"
            ),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(query: &[(&str, &str)]) -> HttpRequest {
        HttpRequest {
            method: "GET".to_string(),
            path: "/api/timeline".to_string(),
            query: query
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
            host: None,
            cookie: None,
        }
    }

    #[test]
    fn absent_means_default_and_out_of_range_is_clamped_policy() {
        assert_eq!(depth_param(&request(&[])).expect("default"), 2);
        assert_eq!(
            depth_param(&request(&[("depth", "99")])).expect("clamped"),
            6
        );
        assert_eq!(
            budget_param(&request(&[("budget", "1")])).expect("clamped"),
            256
        );
        assert_eq!(
            window_param(&request(&[("before", "9999")]), "before").expect("clamped"),
            256
        );
        assert!(depth_param(&request(&[("depth", "abc")])).is_err());
    }

    #[test]
    fn dimension_selection_speaks_the_wake_defaults() {
        assert!(dimension_selection(&request(&[])).is_ok());
        assert!(dimension_selection(&request(&[("dims", "a,b")])).is_ok());
        assert!(dimension_selection(&request(&[("dims", " , ")])).is_err());
        assert!(dimension_selection(&request(&[("scope", "all")])).is_ok());
        assert!(dimension_selection(&request(&[("scope", "galaxy")])).is_err());
    }

    #[test]
    fn tiers_directions_and_axes_refuse_names_outside_their_vocabulary() {
        for tier in ["summary", "spine", "evidence"] {
            assert!(
                tier_param(&request(&[("tier", tier)]))
                    .expect("known")
                    .is_some()
            );
        }
        assert!(tier_param(&request(&[("tier", "gossip")])).is_err());
        for direction in ["goto", "near", "rewind", "forward"] {
            assert!(direction_param(&request(&[("direction", direction)])).is_ok());
        }
        assert!(direction_param(&request(&[("direction", "sideways")])).is_err());
        for axis in ["occurred", "observed", "ingested", "validity"] {
            assert!(axis_param(&request(&[("axis", axis)])).is_ok());
        }
        // `clock` is an accepted spelling of the same parameter.
        assert!(axis_param(&request(&[("clock", "observed")])).is_ok());
        assert!(axis_param(&request(&[("axis", "vibes")])).is_err());
    }

    #[test]
    fn the_temporal_cursor_takes_exactly_one_of_ref_time_seq() {
        assert!(cursor_param(&request(&[("ref", "decision:one")])).is_ok());
        assert!(cursor_param(&request(&[("time", "2026-08-31T00:00:00Z")])).is_ok());
        assert!(cursor_param(&request(&[("seq", "3")])).is_ok());
        assert!(cursor_param(&request(&[("seq", "minus-one")])).is_err());
        assert!(cursor_param(&request(&[])).is_err());
        assert!(
            cursor_param(&request(&[("ref", "a"), ("time", "b")])).is_err(),
            "two cursors are no cursor"
        );
    }
}
