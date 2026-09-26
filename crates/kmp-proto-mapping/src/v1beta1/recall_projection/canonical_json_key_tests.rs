use serde_json::{Value, json};

use super::canonical_json_key;

fn samples() -> Vec<Value> {
    vec![
        json!(null),
        json!(true),
        json!(false),
        json!(0),
        json!(1),
        json!(-1),
        json!(1.0),
        json!(0.0),
        json!(-0.0),
        json!(1.5),
        json!(u64::MAX),
        json!(i64::MIN),
        json!(""),
        json!("a"),
        json!("s1:a"),
        json!("1,"),
        json!([]),
        json!([1, 2]),
        json!([12]),
        json!(["a", "b"]),
        json!(["ab"]),
        json!({}),
        json!({"a": 1, "b": [true, null]}),
        json!({"b": [true, null], "a": 1}),
        json!({"a": "1"}),
        json!({"a1": ""}),
        json!({"a": {"b": "c"}}),
        json!([{"a": 1}, {"a": 1.0}]),
    ]
}

#[test]
fn keys_agree_with_json_equality() {
    let values = samples();
    for left in &values {
        for right in &values {
            assert_eq!(
                canonical_json_key(left) == canonical_json_key(right),
                left == right,
                "{left} vs {right}"
            );
        }
    }
}

#[test]
fn a_reparsed_value_keeps_its_key() {
    for value in samples() {
        let reparsed: Value = serde_json::from_str(&value.to_string()).expect("json");
        assert_eq!(
            value == reparsed,
            canonical_json_key(&value) == canonical_json_key(&reparsed)
        );
    }
}
