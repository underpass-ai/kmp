use super::*;

#[tokio::test]
async fn detail_replay_preserves_cutoff_direction_order_and_admitted_proof() {
    let dir = tempfile::tempdir().expect("isolated store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    seed(&server, "project:fields").await;
    for tool in ["kmp_goto", "kmp_near", "kmp_forward", "kmp_rewind"] {
        let mut args = query();
        args["include"] = json!({"evidence":true,"relations":true,"raw_refs":true});
        if tool == "kmp_goto" {
            args["at"] = json!({"time":"2026-09-01T11:30:00Z"});
        } else if tool == "kmp_near" {
            args["around"] = json!({"time":"2026-09-01T11:00:00Z"});
        }
        let full = call(&server, tool, args.clone()).await;
        assert_eq!(full["entries"].as_array().expect("entries").len(), 2);
        args["fields"] = json!([]);
        let reduced = call(&server, tool, args).await;
        for entry in reduced["entries"].as_array().expect("entries") {
            let action = &entry["detail_action"];
            let expanded = call(
                &server,
                action["tool"].as_str().expect("tool"),
                action["arguments"].clone(),
            )
            .await;
            assert_eq!(
                expanded["temporal"], full["temporal"],
                "{tool}: expanding an old ref must retain the original question's clock/cutoff and traversal"
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
