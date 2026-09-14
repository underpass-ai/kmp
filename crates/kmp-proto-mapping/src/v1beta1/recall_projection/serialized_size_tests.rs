use serde_json::json;

use super::serialized_size::serialized_size;

#[test]
fn counting_writer_matches_serde_json_utf8_bytes() {
    let values = [
        json!(null),
        json!({"empty": [], "false": false, "number": 9_007_199_254_740_993_u64}),
        json!({
            "text": "quote=\" slash=\\ newline=\n tab=\t control=\u{0008} unicode=é界🚀",
            "projection": {
                "budget": {"max_bytes": 4096, "used_bytes": 1024},
                "page": {"offset": 12, "returned": 3, "total": 19, "has_more": true},
                "next_action": {
                    "tool": "kmp_wake",
                    "arguments": {
                        "about": "project:kmp",
                        "page": {"cursor": "kmp1:15:deadbeef"},
                        "budget": {"max_bytes": 4096}
                    }
                }
            }
        }),
    ];

    for value in values {
        assert_eq!(
            serialized_size(&value),
            serde_json::to_vec(&value).expect("oracle JSON").len()
        );
    }
}
