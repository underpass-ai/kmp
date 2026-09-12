use serde_json::json;

#[test]
fn one_repair_removes_all_mixed_search_fields_before_parsing_their_bodies() {
    let mut request = json!({"about":"p","from":"s","to":"t","search":{
        "seek":["verified_by"],"select":{"max_material_nodes":8},
        "paths_per_target":4,"follow":"malformed but incompatible",
        "max_nodes":128,"max_edges":1024,"max_states":2048,"max_depth":64}});
    let error = super::TraceSearchOptionsMapper::from_arguments(&request).expect_err("mixed modes");
    for field in [
        "to",
        "search.follow",
        "search.select",
        "search.paths_per_target",
    ] {
        assert!(error.contains(field), "missing {field}: {error}");
    }
    request.as_object_mut().expect("request").remove("to");
    let search = request["search"].as_object_mut().expect("search");
    for field in ["select", "paths_per_target", "follow"] {
        search.remove(field);
    }
    let (_, targets, options) =
        super::TraceSearchOptionsMapper::from_arguments(&request).expect("one repaired request");
    assert!(targets.is_empty());
    let options = options.expect("search");
    assert!(options.seek.is_some());
    assert_eq!(options.max_nodes, 128);
    assert_eq!(options.max_edges, 1024);
    assert_eq!(options.max_states, 2048);
    assert_eq!(options.max_depth, 64);
}

#[test]
fn every_destination_policy_is_rejected_together_with_seek() {
    let request = json!({"from":"s","search":{"seek":["verified_by"],
        "direction":"outgoing","relations":["verified_by"],"dimensions":{},
        "prefer_dimensions":{},"select":{},"paths_per_target":2,"follow":[]}});
    let error = super::TraceSearchOptionsMapper::from_arguments(&request).expect_err("mixed modes");
    for field in [
        "follow",
        "direction",
        "relations",
        "dimensions",
        "prefer_dimensions",
        "select",
        "paths_per_target",
    ] {
        assert!(
            error.contains(&format!("search.{field}")),
            "{field}: {error}"
        );
    }
}

#[test]
fn destination_options_still_parse_and_unknown_nested_fields_still_fail() {
    let request = json!({"about":"p","from":"s","to":["t"],"search":{
        "paths_per_target":2,"follow":[{"rel":"verified_by","direction":"outgoing"}],
        "select":{"max_material_nodes":8},"dimensions":{"mode":"all"}}});
    let (_, targets, options) =
        super::TraceSearchOptionsMapper::from_arguments(&request).expect("destination mode");
    assert_eq!(targets, ["t"]);
    let options = options.expect("search");
    assert!(options.seek.is_none());
    assert!(options.select.is_some());
    assert_eq!(options.paths_per_target, 2);
    for pointer in ["/search/dimensions", "/search/select", "/search/follow/0"] {
        let mut invalid = request.clone();
        invalid.pointer_mut(pointer).expect("object")["misspelled"] = json!(true);
        assert!(
            crate::contract::validator::reject_unknown_arguments("kmp_trace", &invalid).is_err()
        );
    }
}

#[test]
fn proof_is_shared_by_both_modes_and_requires_a_boolean() {
    for mut request in [
        json!({"about":"p","from":"s","to":["t"],"search":{}}),
        json!({"about":"p","from":"s","search":{"seek":["verified_by"]}}),
    ] {
        assert!(
            !super::TraceSearchOptionsMapper::from_arguments(&request)
                .expect("default")
                .2
                .expect("search")
                .proof
        );
        request["search"]["proof"] = json!(true);
        crate::contract::validator::reject_unknown_arguments("kmp_trace", &request)
            .expect("known option");
        assert!(
            super::TraceSearchOptionsMapper::from_arguments(&request)
                .expect("proof")
                .2
                .expect("search")
                .proof
        );
        request["search"]["proof"] = json!("true");
        assert!(
            super::TraceSearchOptionsMapper::from_arguments(&request)
                .expect_err("typed boundary")
                .contains("search.proof")
        );
    }
}
