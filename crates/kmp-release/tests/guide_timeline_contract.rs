use kmp_release::application::dto::guide_source_dto::GuideSourceDto;
use kmp_release::application::mappers::guide_request_mapper::GuideRequestMapper;
use serde_json::{Value, json};

#[test]
fn guide_positions_roll_through_hours_days_and_months() {
    let entries = (1..=5761)
        .map(|n| {
            json!({
                "id":format!("lesson:{n}"),"kind":"instruction","depth":"basic",
                "text":"One lesson.","evidence":"Authored guide."
            })
        })
        .collect::<Vec<_>>();
    let source: GuideSourceDto = serde_json::from_value(json!({
        "schema_version":1,"guide_version":"1","observed_at":"2026-08-28T00:00:00Z",
        "abouts":[
            {"about":"guide:kmp-agent","audience":"agent","entries":entries,"relations":[]},
            {"about":"guide:kmp","audience":"person","entries":[],"relations":[]}
        ]
    }))
    .expect("source");
    let requests = GuideRequestMapper::map(&source, &[]).expect("requests");
    let entries = &requests[0].body["memory"]["entries"];
    for (sequence, expected) in [
        (1, "2026-08-28T00:00:00Z"),
        (60, "2026-08-28T00:59:00Z"),
        (61, "2026-08-28T01:00:00Z"),
        (1440, "2026-08-28T23:59:00Z"),
        (1441, "2026-08-29T00:00:00Z"),
        (5761, "2026-09-01T00:00:00Z"),
    ] {
        let coordinate = &entries[sequence - 1]["coordinates"][0];
        assert_eq!(coordinate["sequence"], Value::from(sequence));
        assert_eq!(coordinate["occurred_at"], expected);
        assert_eq!(coordinate["observed_at"], expected);
    }
}
