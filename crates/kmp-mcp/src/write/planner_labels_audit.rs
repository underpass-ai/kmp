//! The write planner's labels, audited: the well-known scopes and the
//! caller's own `labels` become coordinates in a fixed order, a key is an
//! identifier a filter can name, a value names one label per write, and a
//! committed write says which labels it created.
#![cfg(test)]

mod tests {
    use crate::write::planner::*;
    use crate::write::planner_audit::tests::sample_write_request;
    use crate::write::results::write_commit_result;
    use serde_json::json;

    #[test]
    fn labels_become_coordinates_under_their_own_key_after_the_well_known_ones() {
        let mut request = sample_write_request();
        request["labels"] = json!({"release": ["v0.12.0"], "component": ["kmp-viewer"]});

        let plan = build_write_plan(&request).expect("labels plan");

        let kinds = plan.ingest_arguments["memory"]["dimensions"]
            .as_array()
            .expect("dimensions")
            .iter()
            .map(|dimension| dimension["kind"].as_str().unwrap_or_default().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                "task",
                "agentic_process",
                "agentic_episode",
                "component",
                "release"
            ],
            "well-known scopes first in their historical order, then the caller's labels by key"
        );
        let coordinates = &plan.ingest_arguments["memory"]["entries"][0]["coordinates"];
        assert_eq!(coordinates.as_array().map(Vec::len), Some(5));
        assert_eq!(coordinates[4]["dimension"], "release");
        assert_eq!(
            coordinates[4]["scope_id"],
            "label:v1:incident%3Amobile-login:release:v0.12.0"
        );
        assert_eq!(
            coordinates[4]["observed_at"], coordinates[1]["observed_at"],
            "every label carries the same clocks"
        );
        assert_eq!(plan.labels.len(), 5);
        assert_eq!(
            plan.labels[3],
            json!({ "key": "component", "value": "kmp-viewer" })
        );
    }

    #[test]
    fn a_label_key_must_be_an_identifier_a_filter_can_name() {
        let mut request = sample_write_request();
        request["labels"] = json!({"Release Train": ["v0.12.0"]});

        let error = build_write_plan(&request).expect_err("bad key");

        assert!(
            error
                .message
                .starts_with("`labels.Release Train` is not a label key"),
            "{error}"
        );
    }

    #[test]
    fn primary_scopes_accept_additional_memberships_but_not_duplicates() {
        let mut request = sample_write_request();
        request["labels"] = json!({"task": ["another-task"], "agentic_process": ["elsewhere"]});
        let plan = build_write_plan(&request).expect("additional memberships");
        assert_eq!(plan.labels.len(), 5);
        request["labels"] = json!({"agentic_process": ["incident:mobile-login:resolution"]});
        let error = build_write_plan(&request).expect_err("exact duplicate");
        assert!(
            error.message.contains("repeat `agentic_process="),
            "{error}"
        );
    }

    #[test]
    fn same_value_under_different_keys_and_multiple_aliases_are_independent() {
        let mut request = sample_write_request();
        request["labels"] = json!({"alias": ["neb", "Nébula Cache"], "component": ["neb"]});
        let plan = build_write_plan(&request).expect("multivalue labels");
        let dimensions = plan.ingest_arguments["memory"]["dimensions"]
            .as_array()
            .expect("dimensions");
        let refs: std::collections::BTreeSet<_> = dimensions
            .iter()
            .map(|d| d["id"].as_str().expect("ref"))
            .collect();
        assert_eq!(refs.len(), dimensions.len());
        assert!(refs.contains("label:v1:incident%3Amobile-login:alias:neb"));
        assert!(refs.contains("label:v1:incident%3Amobile-login:component:neb"));
        assert!(refs.contains("label:v1:incident%3Amobile-login:alias:N%C3%A9bula%20Cache"));
    }

    #[test]
    fn labels_require_nonempty_distinct_arrays() {
        for labels in [
            json!({"alias": "neb"}),
            json!({"alias": []}),
            json!({"alias": [""]}),
            json!({"alias": ["neb", "neb"]}),
            json!({"alias": [42]}),
        ] {
            let mut request = sample_write_request();
            request["labels"] = labels;
            assert!(build_write_plan(&request).is_err(), "{request}");
        }
    }

    #[test]
    fn a_committed_write_says_which_labels_it_created() {
        let mut request = sample_write_request();
        request["labels"] = json!({"release": ["v0.12.0"]});
        request["options"]["dry_run"] = json!(false);
        let plan = build_write_plan(&request).expect("plan");
        let ingest_result = json!({
            "memory": {
                "about": "incident:mobile-login",
                "created_dimensions": [
                    "label:v1:incident%3Amobile-login:release:v0.12.0",
                    "label:v1:incident%3Amobile-login:agentic_episode:incident%3Amobile-login%3Aepisode%3Abackend"
                ]
            }
        });

        let result = write_commit_result(&plan, ingest_result, None, None);

        assert_eq!(
            result["labels"]["written"].as_array().map(Vec::len),
            Some(4)
        );
        assert_eq!(
            result["labels"]["created"],
            json!([
                { "key": "agentic_episode", "value": "incident:mobile-login:episode:backend" },
                { "key": "release", "value": "v0.12.0" }
            ])
        );
    }

    #[test]
    fn strict_writes_ask_the_kernel_to_refuse_a_resembling_label_and_lax_ones_to_warn() {
        let strict = build_write_plan(&sample_write_request()).expect("strict plan");
        assert_eq!(strict.ingest_arguments["label_policy"], "refuse");

        let mut lax = sample_write_request();
        lax["options"]["strict"] = json!(false);
        let lax = build_write_plan(&lax).expect("lax plan");
        assert_eq!(lax.ingest_arguments["label_policy"], "warn");
    }

    #[test]
    fn labels_new_marks_the_dimension_the_writer_insists_on() {
        let mut request = sample_write_request();
        request["labels"] = json!({"component": ["kmp_viewer"]});
        request["options"]["labels_new"] = json!(["component"]);

        let plan = build_write_plan(&request).expect("plan");

        let dimensions = plan.ingest_arguments["memory"]["dimensions"]
            .as_array()
            .expect("dimensions");
        let component = dimensions
            .iter()
            .find(|dimension| dimension["kind"] == "component")
            .expect("component dimension");
        assert_eq!(component["metadata"]["writer_intended_new"], "true");
        assert!(
            dimensions
                .iter()
                .filter(|dimension| dimension["kind"] != "component")
                .all(|dimension| dimension.get("metadata").is_none()),
            "only the insisted label carries the marker"
        );

        let mut request = sample_write_request();
        request["options"]["labels_new"] = json!(["release"]);
        let error = build_write_plan(&request).expect_err("unknown key");
        assert_eq!(
            error.message,
            "options.labels_new names `release`, which is not a label of this write"
        );
    }

    #[test]
    fn a_committed_write_passes_the_kernel_s_resemblances_through() {
        let mut request = sample_write_request();
        request["options"]["dry_run"] = json!(false);
        request["options"]["strict"] = json!(false);
        let plan = build_write_plan(&request).expect("plan");
        let ingest_result = json!({
            "memory": {
                "created_dimensions": [],
                "resembling_labels": [{
                    "key": "task",
                    "value": "incident:mobile-login",
                    "existing_key": "task",
                    "existing_value": "incident-mobile-login",
                    "kind": "same_label_spelled_differently",
                    "why": "`task=incident:mobile-login` resembles `task=incident-mobile-login`, already in the about."
                }]
            }
        });

        let result = write_commit_result(&plan, ingest_result, None, None);

        assert_eq!(
            result["labels"]["resembling"].as_array().map(Vec::len),
            Some(1)
        );
        assert_eq!(
            result["labels"]["resembling"][0]["kind"],
            "same_label_spelled_differently"
        );
        assert_eq!(result["labels"]["created"], json!([]));
    }
}
