use super::*;

fn group(id: &str, source: &str, text: &str) -> ContextGroup {
    ContextGroup {
        id: id.into(),
        spans: vec![],
        reads: vec![
            json!({"tool":"kmp_inspect","arguments":{"about":"project:proof","ref":id,"include":{"raw":true}}}),
        ],
        packets: vec![json!({
            "object":{"ref":id,"kind":"observation","text":text,"metadata":{"source_kind":"original","author":"A","occurred_at":"2026-09-08T10:00:00Z"}},
            "evidence":[{"id":format!("evidence:{id}"),"text":text,"source":source,"time":"2026-09-08T10:01:00Z","supports":[id]}],
            "links":{"outgoing":[{"from":id,"rel":"derived_from","to":source,"why":text,"evidence":text}]},
            "raw":[{"ref":id,"detail":{"text":text,"literal":{"passage":"p1"}}}],
            "page":{"has_more":false,"offset":0,"returned":2,"total":2},
            "warnings":[]
        })],
    }
}

#[test]
fn shared_wording_preserves_distinct_attestations_and_derived_provenance() {
    let text = "No execution is authorized before Tuesday; permission covers R7 only, never R8. "
        .repeat(12);
    let original = group("original", "source:A", &text);
    let mut derived = group("derived", "source:A", &text);
    derived.packets[0]["object"]["metadata"]["source_kind"] = json!("derived");
    derived.packets[0]["object"]["metadata"]["restated_from"] = json!("original");
    let independent = group("independent", "source:B", &text);
    let groups = vec![original, derived, independent];
    let projection = compose(&groups, usize::MAX).expect("valid native fixture");
    assert_eq!(projection["passages"].as_object().expect("object").len(), 1);
    assert_eq!(expand(&projection).expect("valid native fixture"), groups);
    assert_eq!(
        projection["groups"][2]["packets"][0]["evidence"][0]["source"],
        "source:B"
    );
    assert!(projection.to_string().len() < json!(&groups).to_string().len());
    assert_eq!(
        projection,
        compose(&groups, usize::MAX).expect("valid native fixture")
    );
}

#[test]
fn overlapping_quotes_and_distinct_qualifiers_are_never_fused_by_text_guessing() {
    let texts = [
        "On Tuesday only R7 may execute offline. No permission is given for R8.",
        "No permission is given for R8. Confirmation for R7 has not arrived.",
        "Permission is given for R8. Confirmation for R7 has arrived.",
    ];
    let groups = texts
        .iter()
        .enumerate()
        .map(|(i, text)| group(&format!("g{i}"), "source:A", text))
        .collect::<Vec<_>>();
    let result = compose(&groups, usize::MAX).expect("valid native fixture");
    assert_eq!(expand(&result).expect("valid native fixture"), groups);
    // Whole original passages remain distinct, including negation/uncertainty.
    assert_eq!(result["passages"].as_object().expect("object").len(), 3);
}

#[test]
fn whole_groups_survive_budget_admission_and_omissions_have_original_reads() {
    let groups = (0..4)
        .map(|i| {
            group(
                &format!("claim:{i}"),
                &format!("source:{i}"),
                &format!("Source {i}: R7 is allowed only after the separate permission. ")
                    .repeat(30),
            )
        })
        .collect::<Vec<_>>();
    let full = compose(&groups, usize::MAX).expect("valid native fixture");
    let ceiling = full.to_string().len();
    for percent in [100, 75, 50] {
        let budget = ceiling * percent / 100;
        let result = compose(&groups, budget).expect("valid native fixture");
        assert!(result.to_string().len() <= budget);
        let admitted = expand(&result).expect("valid native fixture");
        for g in &admitted {
            assert_eq!(
                g,
                groups
                    .iter()
                    .find(|old| old.id == g.id)
                    .expect("valid native fixture")
            );
        }
        for omitted in result["omitted"].as_array().expect("array") {
            let source = groups
                .iter()
                .find(|g| g.id == omitted["id"])
                .expect("valid native fixture");
            assert_eq!(omitted["reason"], "byte_budget");
            assert_eq!(omitted["reads"], json!(source.reads));
        }
        assert_eq!(
            admitted.len() + result["omitted"].as_array().expect("array").len(),
            groups.len()
        );
    }
}

#[test]
fn a_shared_whole_collection_can_fit_even_when_unshared_single_groups_do_not() {
    let text = "Immutable shared source with a condition that must remain whole. ".repeat(50);
    let groups = vec![group("a", "S1", &text), group("b", "S1", &text)];
    let full = compose(&groups, usize::MAX).expect("valid native fixture");
    assert_eq!(
        expand(&compose(&groups, full.to_string().len()).expect("valid native fixture"))
            .expect("valid native fixture"),
        groups
    );
}

#[test]
fn partial_selection_unknown_conflicts_and_retrieval_warnings_remain_unchanged() {
    let mut g = group("claim", "source:A", &"R7 is not yet verified. ".repeat(25));
    g.packets[0]["answer"] = Value::Null;
    g.packets[0]["page"]["has_more"] = json!(true);
    g.packets[0]["page"]["next_cursor"] = json!("opaque_cursor");
    g.packets[0]["proof"] = json!({"missing":["verification"],"conflicts":[{"from":"source:A","to":"source:B"}],"expired":[{"ref":"old"}]});
    g.packets[0]["warnings"] = json!(["evidence excluded by selection"]);
    let projection = compose(std::slice::from_ref(&g), usize::MAX).expect("valid native fixture");
    assert_eq!(expand(&projection).expect("valid native fixture"), vec![g]);
}

#[test]
fn omission_floor_is_explicit_and_invalid_groups_or_handles_are_rejected() {
    let g = group("claim", "source:A", "R7 is unknown.");
    let floor = compose(std::slice::from_ref(&g), 0).expect("valid native fixture");
    assert_eq!(floor["groups"], json!([]));
    assert_eq!(floor["omitted"][0]["reads"], json!(g.reads));
    assert!(!floor["warnings"].as_array().expect("array").is_empty());
    assert!(compose(&[g.clone(), g.clone()], 10000).is_err());
    let mut encoded = g.clone();
    encoded.packets[0]["object"]["text"] = json!({"passage":"p1"});
    assert!(compose(&[encoded], 10000).is_err());
    let mut invalid = g;
    invalid.reads = vec![json!({"tool":"kmp_inspect","arguments":{"continuation":"read_expired"}})];
    assert!(compose(&[invalid], 10000).is_err());
}

#[test]
fn native_packet_round_trip_keeps_raw_and_opaque_metadata_and_never_grows() {
    for text in [
        "x".to_string(),
        "原文 \"p1\" — do not execute R8. ".repeat(50),
    ] {
        let packet = group("claim", "source:A", &text).packets.remove(0);
        let shared = share_packet(packet.clone());
        assert!(shared.to_string().len() <= packet.to_string().len());
        assert_eq!(shared["raw"], packet["raw"]);
        assert_eq!(share_packet(shared.clone()), shared);
        assert_eq!(expand_packet(shared).expect("valid native fixture"), packet);
    }
    let mut shared = share_packet(
        group("claim", "source:A", &"Long evidence. ".repeat(30))
            .packets
            .remove(0),
    );
    shared["passages"] = json!({});
    assert!(expand_packet(shared).is_err());
}

#[test]
fn citation_handles_resolve_exact_returned_definitions_without_merging_sources() {
    let a = "project:proof:entry:observation:independent-attestation-alpha-0123456789";
    let b = "project:proof:entry:observation:independent-attestation-beta-9876543210";
    let external = "project:proof:entry:constraint:outside-this-packet-01234567890123456789";
    let mut packet = json!({"entries":[{"ref":a,"text":"Identical words","source":"source:A"},{"ref":b,"text":"Identical words","source":"source:B"}],"proof":{"path":(0..8).map(|_|json!({"from":a,"to":b,"rel":"contradicts","evidence_refs":[external]})).collect::<Vec<_>>()}});
    packet["raw"] = json!([{"citation":"c1"}]);
    let shared = share_packet(packet.clone());
    let table = shared["citations"].as_object().expect("citation table");
    assert_eq!(table.len(), 2);
    assert!(table.values().any(|v| v == a));
    assert!(table.values().any(|v| v == b));
    assert!(!table.values().any(|v| v == external));
    assert_eq!(
        shared["entries"], packet["entries"],
        "record definitions stay explicit"
    );
    assert_eq!(
        shared["raw"], packet["raw"],
        "opaque audit data stays literal"
    );
    assert_eq!(expand_packet(shared.clone()).expect("expand refs"), packet);
    let mut broken = shared;
    broken["citations"] = json!({});
    assert!(expand_packet(broken).is_err());
}

#[test]
fn related_fact_definitions_and_tensions_keep_distinct_citation_targets() {
    let a = "project:proof:entry:observation:source-a-with-stable-canonical-identity";
    let b = "project:proof:entry:observation:source-b-with-distinct-canonical-identity";
    let relation = json!({"from":a,"to":b,"rel":"contradicts"});
    let packet = json!({"facts":[{"ref":a,"text":"Allowed."},{"ref":b,"text":"Not allowed."}],
        "declared":[relation.clone(),relation],"tensions":[{"ref":a,"other":b}]});
    let shared = share_packet(packet.clone());
    assert_eq!(
        shared["citations"].as_object().expect("citation map").len(),
        2
    );
    assert_eq!(shared["facts"], packet["facts"]);
    assert_eq!(expand_packet(shared).expect("exact refs"), packet);
}
