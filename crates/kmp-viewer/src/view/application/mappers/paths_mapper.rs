//! Drawn paths: wire ↔ domain.

use crate::view::application::dto::{
    AvoidedHopDto, PathChainDto, PathEndDto, PathHopDto, PathsDto,
};
use crate::view::domain::{
    DrawnPaths, Likelihood, MemoryRef, PathChain, PathEnd, PathStep, ViewError,
};

/// The paths facet of an intent as the view keeps it. A path needs the fact
/// it starts from; everything else may be empty — a search that found no
/// chain is still an answer worth drawing.
pub fn drawn_paths_from_dto(paths: &PathsDto) -> Result<DrawnPaths, ViewError> {
    if paths.from.trim().is_empty() {
        return Err(ViewError::Invalid(
            "a path needs `from`: the fact its chains start from".to_string(),
        ));
    }
    Ok(DrawnPaths {
        from: MemoryRef::new(paths.from.clone()),
        to: paths.to.clone().map(MemoryRef::new),
        max_hops: paths.max_hops,
        review_token: paths.review_token.clone(),
        summary: paths.summary.clone(),
        chains: paths.paths.iter().map(chain_from).collect(),
        avoided: paths.avoided.iter().map(avoided_from).collect(),
        warnings: paths.warnings.clone(),
    })
}

/// The drawn paths as both faces read them back.
pub fn paths_dto(paths: &DrawnPaths) -> PathsDto {
    PathsDto {
        from: paths.from.as_str().to_string(),
        to: paths.to.as_ref().map(|to| to.as_str().to_string()),
        max_hops: paths.max_hops,
        review_token: paths.review_token.clone(),
        summary: paths.summary.clone(),
        paths: paths
            .chains
            .iter()
            .map(|chain| PathChainDto {
                hops: chain.steps.iter().map(hop_dto).collect(),
                proposed: chain.proposed(),
                confidence: chain.likelihood.probability(),
            })
            .collect(),
        avoided: paths
            .avoided
            .iter()
            .map(|step| AvoidedHopDto {
                from: end_dto(&step.from),
                to: end_dto(&step.to),
                rel: step.rel.clone(),
                support: step.likelihood.probability(),
            })
            .collect(),
        warnings: paths.warnings.clone(),
    }
}

fn chain_from(chain: &PathChainDto) -> PathChain {
    PathChain {
        steps: chain.hops.iter().map(step_from).collect(),
        likelihood: Likelihood::from_probability(chain.confidence),
    }
}

fn step_from(hop: &PathHopDto) -> PathStep {
    PathStep {
        from: end_from(&hop.from),
        to: end_from(&hop.to),
        rel: hop.rel.clone(),
        declared: hop.declared,
        reversed: hop.reversed,
        likelihood: if hop.declared {
            Likelihood::CERTAIN
        } else {
            Likelihood::from_probability(hop.confidence)
        },
        // A declared hop has nothing to declare; an id on one would offer
        // the person a write that is already there.
        item_id: hop.item_id.clone().filter(|_| !hop.declared),
    }
}

fn avoided_from(hop: &AvoidedHopDto) -> PathStep {
    PathStep {
        from: end_from(&hop.from),
        to: end_from(&hop.to),
        rel: hop.rel.clone(),
        declared: true,
        reversed: false,
        likelihood: Likelihood::from_probability(hop.support),
        item_id: None,
    }
}

fn end_from(end: &PathEndDto) -> PathEnd {
    PathEnd {
        reference: MemoryRef::new(end.reference.clone()),
        about: end.about.clone(),
        excerpt: end.excerpt.clone(),
    }
}

fn end_dto(end: &PathEnd) -> PathEndDto {
    PathEndDto {
        reference: end.reference.as_str().to_string(),
        about: end.about.clone(),
        excerpt: end.excerpt.clone(),
    }
}

fn hop_dto(step: &PathStep) -> PathHopDto {
    PathHopDto {
        from: end_dto(&step.from),
        to: end_dto(&step.to),
        rel: step.rel.clone(),
        declared: step.declared,
        reversed: step.reversed,
        confidence: step.likelihood.probability(),
        item_id: step.item_id.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn found() -> PathsDto {
        serde_json::from_value(json!({
            "from": "a:1", "to": "a:3", "review_token": "t".repeat(64),
            "summary": "1 paths", "warnings": [],
            "paths": [{"proposed": 1, "confidence": 0.61234, "hops": [
                {"from": {"ref": "a:1", "about": "a", "excerpt": "one"},
                 "to": {"ref": "a:2", "about": "a", "excerpt": "two"},
                 "rel": "causes", "declared": true, "reversed": false, "confidence": 1.0,
                 "item_id": "m9"},
                {"from": {"ref": "a:2", "about": "a", "excerpt": "two"},
                 "to": {"ref": "b:3", "about": "b", "excerpt": "three"},
                 "rel": "same_event_as", "declared": false, "reversed": false,
                 "confidence": 0.61234, "item_id": "m1"}
            ]}],
            "avoided": [{"from": {"ref": "a:1", "about": "a", "excerpt": "one"},
                "to": {"ref": "a:4", "about": "a", "excerpt": "four"},
                "rel": "supports", "support": 0.12}]
        }))
        .expect("curate paths answer")
    }

    #[test]
    fn a_found_path_round_trips_with_its_proposed_hop_declarable() {
        let drawn = drawn_paths_from_dto(&found()).expect("valid");
        assert_eq!(drawn.chains[0].proposed(), 1);
        assert_eq!(
            drawn.chains[0].steps[0].item_id, None,
            "declared: nothing to declare"
        );
        assert_eq!(drawn.chains[0].steps[1].item_id.as_deref(), Some("m1"));
        assert_eq!(drawn.avoided[0].likelihood.probability(), 0.12);
        assert_eq!(
            drawn.refs().iter().map(|r| r.as_str()).collect::<Vec<_>>(),
            ["a:1", "a:2", "b:3", "a:4"]
        );
        let wire = serde_json::to_value(paths_dto(&drawn)).expect("json");
        assert_eq!(wire["paths"][0]["hops"][1]["confidence"], 0.612);
        assert_eq!(wire["paths"][0]["hops"][1]["item_id"], "m1");
        assert!(wire["paths"][0]["hops"][0].get("item_id").is_none());
        assert_eq!(wire["paths"][0]["proposed"], 1);
        assert_eq!(wire["avoided"][0]["support"], 0.12);
        assert_eq!(wire["paths"][0]["hops"][1]["to"]["ref"], "b:3");
    }

    #[test]
    fn a_path_without_its_start_is_refused() {
        let refused = drawn_paths_from_dto(&PathsDto::default());
        assert!(matches!(refused, Err(ViewError::Invalid(message)) if message.contains("from")));
    }
}
