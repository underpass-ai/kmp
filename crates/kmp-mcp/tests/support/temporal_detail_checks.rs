use super::*;

#[tokio::test]
async fn detail_replay_preserves_cutoff_direction_order_and_admitted_proof() {
    let dir = tempfile::tempdir().expect("isolated store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    seed(&server, "project:fields").await;
    for time_move in ["goto", "near", "forward", "rewind"] {
        let mut args = query();
        args["move"] = json!(time_move);
        args["include"] = json!({"evidence":true,"relations":true,"raw_refs":true});
        if time_move == "goto" {
            args["at"] = json!({"time":"2026-09-01T11:30:00Z"});
        } else if time_move == "near" {
            args["around"] = json!({"time":"2026-09-01T11:00:00Z"});
        }
        let full = call(&server, "kmp_time", args.clone()).await;
        assert_eq!(full["entries"].as_array().expect("entries").len(), 2);
        args["fields"] = json!([]);
        let reduced = call(&server, "kmp_time", args).await;
        for entry in reduced["entries"].as_array().expect("entries") {
            let action = &entry["detail_action"];
            assert_eq!(action["tool"], "kmp_time");
            assert_eq!(action["arguments"]["move"], time_move);
            let expanded = call(
                &server,
                action["tool"].as_str().expect("tool"),
                action["arguments"].clone(),
            )
            .await;
            assert_eq!(
                expanded["temporal"], full["temporal"],
                "{time_move}: expanding an old ref must retain the original question's clock/cutoff and traversal"
            );
            assert_eq!(
                expanded["entries"], full["entries"],
                "preserve bodies, placements and ordering"
            );
            assert_eq!(
                expanded["proof"], full["proof"],
                "preserve admitted reasons, not the old ref's earlier knowledge"
            );
            assert_eq!(expanded["raw_refs"], full["raw_refs"]);
        }
    }
}
