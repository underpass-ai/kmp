//! Native dependency records take part in the existing lossless composition.
use super::*;
use sha2::{Digest, Sha256};

fn group() -> ContextGroup {
    let source = "Only the local R19 operation is permitted. Network access is excluded. Both checks are required for completion. ".repeat(45);
    let reference = "project:release:entry:constraint:the-authorized-local-operation-with-two-required-verifications-0123456789abcdef";
    ContextGroup {
        id: "permission-with-source".into(),
        packets: vec![json!({
            "entries":[{"ref":"decision:execute","kind":"decision","text":"Execute R19 locally."}],
            "proof":{
                "entries":[{"ref":reference,"kind":"constraint","text":source,
                    "coordinates":[{"dimension":"source","scope_id":"permit:A","observed_at":"2026-09-01T10:00:00Z"}],
                    "metadata":{"author":"A","opaque":{"passage":"not-a-handle"}}}],
                "groups":[{"seed_ref":"decision:execute","member_refs":["decision:execute",reference],
                    "max_hops":2,"max_members":8,"unavailable_in_selection":1,"omitted_by_limit":2}],
                "evidence":[{"id":"source:A","text":source,"supports":[reference],"source":"permit:A"},
                    {"id":"source:B","text":source,"supports":[reference],"source":"permit:B"}],
                "path":[{"from":"decision:execute","to":reference,"rel":"chosen_because","class":"motivational",
                    "why":"The documented conditions cover this local operation.","evidence":source}],
                "missing":["manifest check"]},
            "page":{"has_more":false},"selection":{"has_more":true},
            "warnings":["More history remains; the group does not establish completion."]})],
        reads: vec![
            json!({"tool":"kmp_goto","arguments":{"about":"project:release",
            "at":{"time":"2026-09-10T10:30:00Z"},"axis":"validity","include":{"dependencies":true}}}),
        ],
        spans: vec![],
    }
}

#[test]
fn shared_dependency_text_and_reference_uses_restore_with_all_qualifiers() {
    let group = group();
    let original = &group.packets[0];
    let shared = share_packet(original.clone());
    assert!(shared["proof"]["entries"][0]["text"]["passage"].is_string());
    assert!(shared["proof"]["path"][0]["to"]["citation"].is_string());
    // Membership remains canonical even when other ref uses can be shortened.
    assert_eq!(shared["proof"]["groups"], original["proof"]["groups"]);
    assert_eq!(
        shared["proof"]["entries"][0]["metadata"],
        original["proof"]["entries"][0]["metadata"]
    );
    assert!(shared.to_string().len() < original.to_string().len());
    assert_eq!(expand_packet(shared).expect("same-page tables"), *original);
    let context = compose(std::slice::from_ref(&group), usize::MAX).expect("compose");
    assert_eq!(expand(&context).expect("context tables"), vec![group]);
}

#[test]
fn dependency_definition_resolves_known_spans_without_a_duplicate_source_record() {
    let mut group = group();
    let source = group.packets[0]["proof"]["entries"][0]["text"]
        .as_str()
        .expect("source")
        .to_string();
    let reference = group.packets[0]["proof"]["entries"][0]["ref"]
        .as_str()
        .expect("ref")
        .to_string();
    group.packets[0]["proof"]["evidence"] = json!([
        {"id":"quote:1","text":&source[0..2000],"source":"permit:A"},
        {"id":"quote:2","text":&source[1000..3000],"source":"permit:A"}]);
    group.packets[0]["proof"]["path"] = json!([]);
    for (index, start, end) in [(0, 0, 2000), (1, 1000, 3000)] {
        group.spans.push(SourceSpan {
            packet: 0,
            pointer: format!("/proof/evidence/{index}/text"),
            source_ref: reference.clone(),
            source_sha256: format!("{:x}", Sha256::digest(source.as_bytes())),
            start_utf8: start,
            end_utf8: end,
        });
    }
    let context = compose(std::slice::from_ref(&group), usize::MAX)
        .expect("dependency is the returned source");
    assert!(
        context["groups"][0]["packets"][0]["proof"]["entries"][0]["text"]["passage"].is_array()
    );
    assert_eq!(
        expand(&context).expect("reconstruct whole dependency"),
        vec![group]
    );
}

#[test]
fn omitted_dependency_group_cannot_leak_bodies_from_its_passage_table() {
    let group = group();
    let projected = compose(std::slice::from_ref(&group), 1).expect("manifest floor");
    assert!(expand(&projected).expect("no admitted group").is_empty());
    assert_eq!(projected["omitted"][0]["reads"], json!(group.reads));
    assert_eq!(projected["omitted"][0]["id"], group.id);
    assert!(!projected.to_string().contains("Network access is excluded"));
}
