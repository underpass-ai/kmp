//! The writer's identity, audited: refs generated stably across retries,
//! never colliding, always safe descendants of their about; idempotency
//! that ignores the dry-run switch.
#![cfg(test)]

mod tests {
    #[allow(unused_imports)]
    use crate::write::generated_ref::{GENERATED_REF_HASH_LEN, GENERATED_REF_SEGMENT_MAX};
    #[allow(unused_imports)]
    use crate::write::planner::*;
    #[allow(unused_imports)]
    use serde_json::{Map, Value, json};

    fn sample_write_request() -> Value {
        json!({
            "about": "incident:mobile-login",
            "intent": "record_decision",
            "actor": "agent:backend",
            "observed_at": "2026-05-06T10:00:00Z",
            "scope": {
                "task": "incident:mobile-login",
                "process": "incident:mobile-login:resolution",
                "episode": "incident:mobile-login:episode:backend"
            },
            "current": {
                "kind": "decision",
                "summary": "Use token refresh retry instead of widening timeout.",
                "evidence": "Logs show 401 immediately after token refresh."
            },
            "semantic_delta": {
                "from": "The team suspected network timeout.",
                "to": "The evidence points to token refresh race.",
                "why": "The failing requests return 401 immediately after refresh.",
                "evidence": "Auth logs show refresh success followed by 401 on the next request."
            },
            "connect_to": [
                {
                    "ref": "incident:mobile-login:observation:401-refresh-race",
                    "rel": "chosen_because",
                    "class": "causal",
                    "why": "The decision addresses the observed token refresh race.",
                    "evidence": "The chosen retry targets the refresh race seen in auth logs."
                }
            ],
            "read_context": {
                "inspected_refs": [
                    "incident:mobile-login:observation:401-refresh-race"
                ]
            },
            "options": {
                "dry_run": true,
                "strict": true
            }
        })
    }
    #[test]
    fn stable_idempotency_ignores_dry_run_switch() {
        let mut commit = sample_write_request();
        commit["options"]["dry_run"] = json!(false);

        let dry_plan = build_write_plan(&sample_write_request()).expect("dry run should plan");
        let commit_plan = build_write_plan(&commit).expect("commit should plan");

        assert_eq!(
            dry_plan.ingest_arguments["idempotency_key"],
            commit_plan.ingest_arguments["idempotency_key"]
        );
        assert_ne!(
            dry_plan.ingest_arguments["dry_run"],
            commit_plan.ingest_arguments["dry_run"]
        );
        assert_eq!(
            dry_plan.generated_refs, commit_plan.generated_refs,
            "previewing and committing the same logical write must name the same entries"
        );
    }
    #[test]
    fn generated_refs_distinguish_repeated_observations_with_the_same_summary() {
        let mut morning = sample_write_request();
        morning
            .as_object_mut()
            .expect("request")
            .remove("semantic_delta");
        morning["current"]["kind"] = json!("observation");
        morning["current"]["summary"] = json!("la presion del circuito es normal");
        morning["current"]["evidence"] = json!("manometro a las 09:00 marca 4.1 bar");
        morning["occurred_at"] = json!("2026-05-06T09:00:00Z");

        let mut afternoon = morning.clone();
        afternoon["current"]["evidence"] = json!("manometro a las 17:00 marca 4.0 bar");
        afternoon["occurred_at"] = json!("2026-05-06T17:00:00Z");

        let morning_ref = build_write_plan(&morning)
            .expect("morning observation")
            .generated_refs[0]
            .clone();
        let afternoon_ref = build_write_plan(&afternoon)
            .expect("afternoon observation")
            .generated_refs[0]
            .clone();

        assert_ne!(morning_ref, afternoon_ref);
        for generated_ref in [morning_ref, afternoon_ref] {
            let suffix = generated_ref.rsplit(':').next().expect("ref suffix");
            assert!(
                suffix.starts_with("la-presion-del-circuito-es-normal-"),
                "keep the ref readable: {generated_ref}"
            );
            assert!(
                suffix.len() <= GENERATED_REF_SEGMENT_MAX,
                "the identity hash must fit inside the existing segment bound: {generated_ref}"
            );
        }
    }
    #[test]
    fn generated_refs_distinguish_opposite_long_summaries_after_the_shared_prefix() {
        let prefix =
            "el despliegue de la version 2.4.1 en el entorno de preproduccion ha terminado con ";
        let mut success = sample_write_request();
        success
            .as_object_mut()
            .expect("request")
            .remove("semantic_delta");
        success["current"]["kind"] = json!("observation");
        success["current"]["summary"] = json!(format!("{prefix}exito y sin incidencias"));
        success["current"]["evidence"] = json!("CI job 4001: exit 0");

        let mut failure = success.clone();
        failure["current"]["summary"] = json!(format!("{prefix}errores graves de arranque"));
        failure["current"]["evidence"] = json!("CI job 4002: exit 1, panic en el arranque");

        let success_ref = build_write_plan(&success)
            .expect("successful deployment observation")
            .generated_refs[0]
            .clone();
        let failure_ref = build_write_plan(&failure)
            .expect("failed deployment observation")
            .generated_refs[0]
            .clone();

        assert_ne!(success_ref, failure_ref);
        let success_suffix = success_ref.rsplit(':').next().expect("success suffix");
        let failure_suffix = failure_ref.rsplit(':').next().expect("failure suffix");
        let success_slug = success_suffix.rsplit_once('-').expect("slug and hash").0;
        let failure_slug = failure_suffix.rsplit_once('-').expect("slug and hash").0;
        assert_eq!(
            success_slug, failure_slug,
            "this regression must prove the hash, not the readable prefix, separates the writes"
        );
        assert!(success_suffix.len() <= GENERATED_REF_SEGMENT_MAX);
        assert!(failure_suffix.len() <= GENERATED_REF_SEGMENT_MAX);
    }
    #[test]
    fn generated_refs_keep_non_ascii_summaries_retry_stable_and_write_unique() {
        let mut first = sample_write_request();
        first
            .as_object_mut()
            .expect("request")
            .remove("semantic_delta");
        first["current"]["kind"] = json!("observation");
        first["current"]["summary"] = json!("配管の圧力は正常");
        first["current"]["evidence"] = json!("圧力計は4.1 bar");
        first["occurred_at"] = json!("2026-05-06T09:00:00Z");

        let retry_ref = build_write_plan(&first)
            .expect("first non-ASCII observation")
            .generated_refs[0]
            .clone();
        assert_eq!(
            retry_ref,
            build_write_plan(&first)
                .expect("exact retry")
                .generated_refs[0]
        );

        let mut second = first.clone();
        second["current"]["evidence"] = json!("圧力計は4.0 bar");
        second["occurred_at"] = json!("2026-05-06T17:00:00Z");
        let second_ref = build_write_plan(&second)
            .expect("second non-ASCII observation")
            .generated_refs[0]
            .clone();

        assert_ne!(retry_ref, second_ref);
        assert_eq!(
            retry_ref.rsplit(':').next().expect("hash suffix").len(),
            GENERATED_REF_HASH_LEN
        );
        assert_eq!(
            second_ref.rsplit(':').next().expect("hash suffix").len(),
            GENERATED_REF_HASH_LEN
        );
    }
    #[test]
    fn rejects_duplicate_generated_refs() {
        let mut request = sample_write_request();
        let current_ref = "incident:mobile-login:entry:decision:duplicate";
        request["current"]["ref"] = json!(current_ref);
        request["semantic_delta"]["ref"] = json!(current_ref);

        let error = build_write_plan(&request).expect_err("duplicate refs should fail");

        assert_eq!(
            error,
            "generated duplicate memory ref `incident:mobile-login:entry:decision:duplicate`"
        );
    }
    #[test]
    fn supplied_entry_ref_must_be_a_safe_descendant_of_its_about() {
        let unsafe_refs = [
            "incident:other:entry:observation:foreign",
            "incident:other",
            "evidence:incident:other:entry:observation:foreign:current",
            "about:incident:other:dimension:shared",
            "../../incident:other:entry:observation:foreign",
            "incident:mobile-login:entry:x\nincident:other:entry:y",
        ];

        for (object, path) in [
            ("current", "current.ref"),
            ("semantic_delta", "semantic_delta.ref"),
        ] {
            for unsafe_ref in unsafe_refs {
                let mut request = sample_write_request();
                request[object]["ref"] = json!(unsafe_ref);
                let error = build_write_plan(&request).expect_err("unsafe ref must be refused");
                assert!(
                    error.contains("does not belong to about")
                        || error.contains(&format!("invalid `{path}`")),
                    "the refusal must name `{path}` and its violated boundary for `{unsafe_ref}`: {error}"
                );
                assert!(error.contains(path), "wrong supplied-ref path: {error}");
            }
        }

        let mut local_current = sample_write_request();
        local_current["current"]["ref"] =
            json!("incident:mobile-login:decision:explicit-safe-update");
        let plan = build_write_plan(&local_current).expect("an about-local current ref is valid");
        assert_eq!(
            plan.generated_refs[0],
            "incident:mobile-login:decision:explicit-safe-update"
        );

        let mut local_delta = sample_write_request();
        local_delta["semantic_delta"]["ref"] =
            json!("incident:mobile-login:semantic-delta:explicit-safe-update");
        let plan = build_write_plan(&local_delta).expect("an about-local delta ref is valid");
        assert_eq!(
            plan.generated_refs[1],
            "incident:mobile-login:semantic-delta:explicit-safe-update"
        );
    }
    #[test]
    fn unsafe_about_ref_is_rejected_before_it_can_namespace_generated_nodes() {
        for unsafe_about in [
            "../../incident:mobile-login",
            "incident:mobile-login\nincident:other",
            "incident::mobile-login",
        ] {
            let mut request = sample_write_request();
            request["about"] = json!(unsafe_about);
            let error = build_write_plan(&request).expect_err("unsafe about must be refused");
            assert!(error.contains("invalid `about`"), "{error}");
        }
    }
}
