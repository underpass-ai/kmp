//! One kind of absence, two deliberate answers, audited on the advertised
//! surface so neither is an open question again.
//!
//! [#443](https://github.com/underpass-ai/kmp/issues/443) asked whether
//! `kmp_view_open` and `kmp_view_apply_intent` should answer a ref that is
//! not in the store the same way, and split them. `kmp_view_open` fails,
//! because every pane of the loom it would draw is that about. An intent
//! degrades, because it moves a loom already open over memory that is really
//! there, and it names what it dropped through the `unhonored` channel its
//! dimensions and overlays already use. The behavior is pinned by the
//! embedded tests; what is pinned here is that the surface an agent reads
//! says both — an agent that is told only one of them learns the wrong rule.
#![cfg(test)]

use serde_json::Value;

use crate::contract::registry::tools_list_result;

#[test]
fn absence_fails_an_open_and_degrades_an_intent_on_the_advertised_surface() {
    let result = tools_list_result();
    let tools = result["tools"].as_array().expect("tools");
    let view = |name: &str| {
        tools
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap_or_else(|| panic!("`{name}` is not advertised"))
            .clone()
    };
    let text = |value: &Value| value.as_str().expect("description").to_string();

    let about = text(&view("kmp_view_open")["inputSchema"]["properties"]["about"]["description"]);
    assert!(
        about.contains("must exist") && about.contains("empty loom"),
        "kmp_view_open must keep saying an absent about fails, and why: {about}"
    );

    let intent = view("kmp_view_apply_intent");
    assert!(
        text(&intent["description"]).contains("unhonored"),
        "kmp_view_apply_intent must advertise where absence is reported"
    );
    let properties = &intent["inputSchema"]["properties"];
    for pointer in [
        "/target/properties/about/description",
        "/focus/properties/refs/description",
        "/projection/properties/abouts/items/description",
        "/selection/description",
        "/trace/description",
    ] {
        let note = text(properties.pointer(pointer).unwrap_or_else(|| {
            panic!("`{pointer}` must describe what happens to a ref that is not in the store")
        }));
        assert!(
            note.contains("unhonored") && note.contains("this store"),
            "every part of an intent that names a ref must say absence is dropped and named: \
             {pointer} says {note}"
        );
    }
    assert!(
        text(&intent["outputSchema"]["properties"]["unhonored"]["description"])
            .contains("never silently drawn"),
        "the unhonored channel must promise that absence is never silent"
    );
    assert!(
        text(&intent["outputSchema"]["properties"]["applied"]["description"])
            .contains("none of whose named refs are in this store"),
        "an intent nothing could honor must be distinguishable from one that moved the view"
    );
}
