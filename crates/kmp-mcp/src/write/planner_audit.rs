//! The write planner, audited end to end: strict-mode policy, generated
//! refs, canonical coordinates, idempotency, and every refusal a caller
//! can fix.
#![cfg(test)]

mod tests {
    #[allow(unused_imports)]
    use crate::write::generated_ref::{GENERATED_REF_HASH_LEN, GENERATED_REF_SEGMENT_MAX};
    #[allow(unused_imports)]
    use crate::write::plan::KernelWritePlan;
    #[allow(unused_imports)]
    use crate::write::planner::*;
    #[allow(unused_imports)]
    use crate::write::results::{write_commit_result, write_dry_run_result};
    #[allow(unused_imports)]
    use serde_json::{Map, Value, json};

    #[test]
    fn write_commits_unless_a_preview_is_requested() {
        // No `options` at all is the shape a caller reaches for first, and it
        // must commit: previewing by default meant `isError: false` on a call
        // that wrote nothing.
        let mut request = sample_write_request();
        request.as_object_mut().expect("object").remove("options");
        let plan = build_write_plan(&request).expect("write should plan");
        assert!(
            !plan.dry_run,
            "write_memory must commit by default; previewing is opt-in"
        );

        let mut strict_only = sample_write_request();
        strict_only["options"] = json!({ "strict": true });
        let plan = build_write_plan(&strict_only).expect("write should plan");
        assert!(
            !plan.dry_run,
            "setting an unrelated option must not turn a write into a preview"
        );
    }

    #[test]
    fn dry_run_generates_canonical_ingest_preview() {
        let plan = build_write_plan(&sample_write_request()).expect("write should plan");

        assert!(plan.dry_run);
        assert_eq!(plan.generated_refs.len(), 2);
        assert_eq!(
            plan.relations,
            vec!["chosen_because", "updates_state", "semantic_delta_from"]
        );
        assert_eq!(plan.relation_quality.len(), 3);
        assert_eq!(
            plan.relation_quality_metrics["relation_rich_count"],
            json!(3)
        );
        assert_eq!(
            plan.relation_quality_metrics["relation_anemic_count"],
            json!(0)
        );
        assert_eq!(
            plan.relation_quality_metrics["relation_prior_context_required_count"],
            json!(2)
        );
        assert_eq!(
            plan.relation_quality_metrics["relation_prior_context_coverage"],
            json!(1.0)
        );
        assert_eq!(plan.relation_quality[0]["quality"], json!("rich"));
        assert_eq!(
            plan.relation_quality[0]["prior_context_sources"],
            json!(["kmp_inspect"])
        );
        assert_eq!(plan.ingest_arguments["about"], "incident:mobile-login");
        assert_eq!(
            plan.ingest_arguments["memory"]["dimensions"][0]["kind"],
            "task"
        );
        assert_eq!(
            plan.ingest_arguments["memory"]["dimensions"][1]["kind"],
            "agentic_process"
        );
        assert_eq!(
            plan.ingest_arguments["memory"]["entries"][1]["kind"],
            "semantic_delta"
        );
        assert_eq!(
            plan.ingest_arguments["memory"]["relations"][0]["rel"],
            "chosen_because"
        );
        assert_eq!(
            plan.ingest_arguments["memory"]["relations"][2]["rel"],
            "semantic_delta_from"
        );
        assert_eq!(
            plan.ingest_arguments["provenance"]["source_agent"],
            "agent:backend"
        );
    }

    #[test]
    fn writer_carries_every_known_clock_into_canonical_coordinates() {
        let mut request = sample_write_request();
        request["occurred_at"] = json!("2026-05-06T09:58:00Z");
        request["valid_from"] = json!("2026-05-06T10:00:00Z");
        request["valid_until"] = json!("2026-05-07T10:00:00Z");
        request["rank"] = json!(7);

        let plan = build_write_plan(&request).expect("polytemporal write should plan");
        let coordinates = plan.ingest_arguments["memory"]["entries"][0]["coordinates"]
            .as_array()
            .expect("entry coordinates");

        assert!(!coordinates.is_empty());
        for coordinate in coordinates {
            assert_eq!(coordinate["occurred_at"], "2026-05-06T09:58:00Z");
            assert_eq!(coordinate["observed_at"], "2026-05-06T10:00:00Z");
            assert_eq!(coordinate["valid_from"], "2026-05-06T10:00:00Z");
            assert_eq!(coordinate["valid_until"], "2026-05-07T10:00:00Z");
            assert_eq!(coordinate["rank"], 7);
        }

        let shifted = plan.ingest_arguments["memory"]["entries"][1]["coordinates"]
            .as_array()
            .expect("semantic delta coordinates");
        assert_eq!(shifted[0]["occurred_at"], "2026-05-06T09:58:00Z");
        assert_eq!(shifted[0]["rank"], 7);
    }

    #[test]
    fn structural_links_compile_without_an_empty_rationale() {
        // The first memory in a store is often a plain `scoped_to` into its
        // own about, with no rationale to give — structural links are exempt
        // from why and evidence by this tool's own contract. Compiling that
        // exemption to `"why": ""` made the canonical ingest mapper reject the
        // write as a malformed argument, so the exemption existed on paper
        // only.
        let mut request = sample_write_request();
        request
            .as_object_mut()
            .expect("sample request should be an object")
            .remove("semantic_delta");
        request["connect_to"] = json!([{
            "ref": "incident:mobile-login",
            "rel": "scoped_to",
            "class": "structural"
        }]);

        let plan = build_write_plan(&request).expect("a structural link needs no rationale");

        let relation = &plan.ingest_arguments["memory"]["relations"][0];
        assert_eq!(relation["rel"], "scoped_to");
        assert!(
            relation.get("why").is_none(),
            "an absent why must stay absent, not become an empty string: {relation}"
        );
        assert!(
            relation.get("evidence").is_none(),
            "an absent evidence must stay absent, not become an empty string: {relation}"
        );

        let evidence = plan.ingest_arguments["memory"]["evidence"]
            .as_array()
            .expect("evidence should be an array");
        assert_eq!(
            evidence.len(),
            1,
            "only the entry's own evidence survives; a structural link contributes none: {evidence:?}"
        );
        assert_eq!(
            evidence[0]["text"],
            "Logs show 401 immediately after token refresh."
        );
    }

    #[test]
    fn rejects_missing_process_scope() {
        let mut request = sample_write_request();
        request["scope"]
            .as_object_mut()
            .expect("sample scope should be an object")
            .remove("process");

        let error = build_write_plan(&request).expect_err("process scope is required");

        assert_eq!(error, "missing required argument `scope.process`");
    }

    #[test]
    fn rejects_scope_ids_reused_across_dimensions_before_ingest() {
        let mut request = sample_write_request();
        request["scope"]["task"] = json!("incident:mobile-login:resolution");

        let error = build_write_plan(&request).expect_err("scope ids must be distinct");

        assert_eq!(
            error,
            "scope.process and scope.task reuse `incident:mobile-login:resolution`; within an about a scope id names one label and keeps the kind of its first use, so one id cannot be two kinds"
        );
    }

    #[test]
    fn omitted_writer_sequence_stays_absent_for_kernel_assignment() {
        let plan = build_write_plan(&sample_write_request()).expect("write should plan");
        let entries = plan.ingest_arguments["memory"]["entries"]
            .as_array()
            .expect("entries");

        assert!(entries.iter().all(|entry| {
            entry["coordinates"]
                .as_array()
                .expect("coordinates")
                .iter()
                .all(|coordinate| coordinate.get("sequence").is_none())
        }));
    }

    #[test]
    fn rejects_relation_without_evidence_in_strict_shape() {
        let mut request = sample_write_request();
        request["connect_to"][0]
            .as_object_mut()
            .expect("sample relation should be an object")
            .remove("evidence");

        let error = build_write_plan(&request).expect_err("relation evidence is required");

        assert_eq!(error, "missing required argument `connect_to[0].evidence`");
    }

    #[test]
    fn rejects_strict_write_without_any_relation_after_the_about_exists() {
        let mut request = sample_write_request();
        request
            .as_object_mut()
            .expect("sample request should be an object")
            .remove("connect_to");

        let error = build_write_plan(&request).expect_err("strict write requires a relation");

        assert_eq!(
            error,
            "strict kmp_write_memory requires at least one connect_to relation once the about exists; inspect or traverse a target first, or set options.strict=false when an unlinked write is intentional"
        );
    }

    #[test]
    fn accepts_the_first_strict_write_as_an_unlinked_about_root() {
        let mut request = sample_write_request();
        request
            .as_object_mut()
            .expect("sample request should be an object")
            .remove("connect_to");

        let plan = build_write_plan_with_root(&request, true)
            .expect("a server-proven new about may form its root");
        assert!(
            !plan
                .relations
                .iter()
                .any(|relation| relation == "chosen_because"),
            "the plan succeeds without inventing a link from the root"
        );
    }

    #[test]
    fn rejects_rich_relation_without_read_context_in_strict_mode() {
        let mut request = sample_write_request();
        request
            .as_object_mut()
            .expect("sample request should be an object")
            .remove("read_context");

        let error = build_write_plan(&request).expect_err("rich relation requires prior read");

        assert_eq!(
            error,
            "strict kmp_write_memory rich relation `chosen_because` to `incident:mobile-login:observation:401-refresh-race` requires read_context evidence; inspect, trace, or traverse the target first, or use an explicit anemic fallback"
        );
    }

    #[test]
    fn rejects_self_loop_relations() {
        let mut request = sample_write_request();
        let current_ref = "incident:mobile-login:entry:decision:self";
        request["current"]["ref"] = json!(current_ref);
        request["connect_to"][0]["ref"] = json!(current_ref);

        let error = build_write_plan(&request).expect_err("self-loop should fail");

        assert_eq!(
            error,
            "kmp_write_memory relation `chosen_because` cannot point from and to the same ref `incident:mobile-login:entry:decision:self`"
        );
    }

    #[test]
    fn classifies_explicit_anemic_fallback_relations() {
        let mut request = sample_write_request();
        request
            .as_object_mut()
            .expect("sample request should be an object")
            .remove("semantic_delta");
        request
            .as_object_mut()
            .expect("sample request should be an object")
            .remove("read_context");
        request["connect_to"][0]["rel"] = json!("follows");
        request["connect_to"][0]["class"] = json!("procedural");
        request["connect_to"][0]["why"] =
            json!("The new turn follows this prior process turn in sequence.");
        request["connect_to"][0]["evidence"] =
            json!("The writer only knows process succession for this memory.");

        let plan = build_write_plan(&request).expect("anemic fallback should be explicit");

        assert_eq!(plan.relation_quality.len(), 1);
        assert_eq!(plan.relation_quality[0]["quality"], "anemic");
        assert_eq!(plan.relation_quality[0]["fallback"], true);
        assert_eq!(
            plan.relation_quality_metrics["relation_anemic_count"],
            json!(1)
        );
    }

    #[test]
    fn accepts_core_operand_modeling_relations() {
        let mut request = sample_write_request();
        request["connect_to"][0]["rel"] = json!("matches_question_item");
        request["connect_to"][0]["class"] = json!("constraint");
        request["connect_to"][0]["why"] =
            json!("The prior memory satisfies the current requirement predicate.");
        request["connect_to"][0]["evidence"] =
            json!("The writer observed the target ref before linking it.");

        let plan = build_write_plan(&request).expect("operand relation should be accepted");

        assert_eq!(plan.relation_quality[0]["quality"], "rich");
        assert_eq!(
            plan.ingest_arguments["memory"]["relations"][0]["rel"],
            "matches_requirement"
        );
    }

    #[test]
    fn rejects_unsupported_relations_in_strict_mode() {
        let mut request = sample_write_request();
        request["connect_to"][0]["rel"] = json!("related_to");

        let error = build_write_plan(&request).expect_err("vague relation should fail");

        assert_eq!(
            error,
            "unsupported or vague kmp_write_memory relation `related_to`"
        );
    }

    /// A mutation probe showed nothing pinned the non-strict demotion:
    /// a rich relation written without read-context evidence must arrive
    /// as `suspect`, or lax mode silently launders unaudited certainty.
    #[test]
    fn a_rich_relation_without_read_context_is_suspect_when_strict_is_off() {
        let mut request = sample_write_request();
        request["options"] = json!({"strict": false});
        request
            .as_object_mut()
            .expect("request object")
            .remove("read_context");

        let plan = build_write_plan(&request).expect("non-strict write is accepted");
        let quality = plan.relation_quality[0]["quality"]
            .as_str()
            .expect("relation quality");
        assert_eq!(quality, "suspect");
        assert!(
            plan.relation_quality[0]["quality_reason"]
                .as_str()
                .expect("reason")
                .contains("must be audited"),
            "{:?}",
            plan.relation_quality[0]
        );
    }

    /// A memory written in Spanish reaches an English question only through
    /// its rendering, so a strict write without one is refused with the
    /// reason, and one with a rendering stores it beside the text.
    #[test]
    fn a_strict_write_of_a_spanish_memory_requires_and_stores_its_english_summary() {
        let mut request = sample_write_request();
        request["current"]["summary"] =
            json!("El despliegue de v0.7.0 se retrasó porque los auditores no firmaron.");

        let error = build_write_plan(&request).expect_err("no rendering, no strict write");
        assert!(error.contains("requires current.summary_en"), "{error}");
        assert!(error.contains("leans to spanish"), "{error}");

        request["current"]["summary_en"] =
            json!("The v0.7.0 launch was postponed because the auditors had not signed off.");
        let plan = build_write_plan(&request).expect("a faithful rendering is accepted");
        let entry = &plan.ingest_arguments["memory"]["entries"][0];
        assert_eq!(
            entry["text"],
            "El despliegue de v0.7.0 se retrasó porque los auditores no firmaron."
        );
        assert_eq!(
            entry["metadata"]["summary_en"],
            "The v0.7.0 launch was postponed because the auditors had not signed off."
        );
        assert!(plan.diagnostics.is_empty(), "{:?}", plan.diagnostics);
    }

    #[test]
    fn a_strict_write_refuses_a_summary_that_fails_the_lint_and_names_the_fault() {
        let mut request = sample_write_request();
        request["current"]["summary"] =
            json!("El despliegue de v0.7.0 se retrasó porque los auditores no firmaron.");
        request["current"]["summary_en"] =
            json!("The launch was postponed because the auditors had not signed off.");

        let error = build_write_plan(&request).expect_err("a dropped identifier is refused");

        assert!(error.contains("refuses current.summary_en"), "{error}");
        assert!(
            error.contains("drops identifiers the text carries: v0.7.0"),
            "{error}"
        );
    }

    #[test]
    fn an_english_memory_needs_no_summary_and_keeps_one_it_is_given() {
        let request = sample_write_request();
        let plan = build_write_plan(&request).expect("English text is reached in English");
        assert!(
            plan.ingest_arguments["memory"]["entries"][0]["metadata"]
                .get("summary_en")
                .is_none()
        );

        let mut request = sample_write_request();
        request["current"]["summary_en"] =
            json!("Retry the token refresh instead of widening the timeout.");
        let plan = build_write_plan(&request).expect("a rendering of English text is accepted");
        assert_eq!(
            plan.ingest_arguments["memory"]["entries"][0]["metadata"]["summary_en"],
            "Retry the token refresh instead of widening the timeout."
        );
    }

    #[test]
    fn outside_strict_mode_a_failing_summary_is_stored_and_the_plan_says_it_will_not_carry() {
        let mut request = sample_write_request();
        request["options"]["strict"] = json!(false);
        request["current"]["summary"] =
            json!("El despliegue de v0.7.0 se retrasó porque los auditores no firmaron.");
        request["current"]["summary_en"] = json!("Se pospuso el despliegue.");

        let plan = build_write_plan(&request).expect("non-strict stores what it is given");

        assert_eq!(
            plan.ingest_arguments["memory"]["entries"][0]["metadata"]["summary_en"],
            "Se pospuso el despliegue."
        );
        assert_eq!(plan.diagnostics.len(), 1, "{:?}", plan.diagnostics);
        assert!(
            plan.diagnostics[0].contains("will not carry retrieval"),
            "{:?}",
            plan.diagnostics
        );
        assert!(
            plan.diagnostics[0].contains("leans to spanish, not to English"),
            "{:?}",
            plan.diagnostics
        );
    }

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

    fn cross_about_request() -> Value {
        let mut request = sample_write_request();
        // A declaration of identity carries no delta of its own: the delta
        // relation would point at the other about too, and only the
        // equivalence may.
        request
            .as_object_mut()
            .expect("request object")
            .remove("semantic_delta");
        request["connect_to"] = json!([{
            "ref": "incident:platform:outcome:freeze",
            "rel": "same_event_as",
            "class": "evidential",
            "why": "Both record the same freeze.",
            "evidence": "kmp_relate proposal by identifier.",
            "confidence": "high"
        }]);
        request["read_context"] = json!({
            "relate_proposals": [{
                "from": "incident:mobile-login:observation:401-refresh-race",
                "to": "incident:platform:outcome:freeze",
                "proposed_by": ["identifier", "entity"]
            }]
        });
        request["options"] = json!({"dry_run": true, "strict": true});
        request
    }

    /// The one relation that crosses an about: declared from a proposal,
    /// stamped with it, its evidence claiming only what this about owns.
    #[test]
    fn an_equivalence_to_another_about_is_declared_from_its_proposal() {
        let plan = build_write_plan_with_root(&cross_about_request(), false)
            .expect("a declared equivalence is accepted");
        let relation = &plan.ingest_arguments["memory"]["relations"][0];
        assert_eq!(relation["rel"], "same_event_as");
        assert_eq!(relation["to"], "incident:platform:outcome:freeze");
        assert_eq!(relation["method"], "kmp_relate:identifier+entity");
        let evidence = &plan.ingest_arguments["memory"]["evidence"][0];
        assert_eq!(
            evidence["supports"].as_array().map(Vec::len),
            Some(1),
            "the evidence node claims this about's entry, not the other about's: {evidence}"
        );
        assert_eq!(plan.relation_quality[0]["crosses_about"], true);
        assert_eq!(
            plan.relation_quality[0]["prior_context_sources"],
            json!(["kmp_relate"])
        );
    }

    #[test]
    fn any_other_relation_across_abouts_and_an_unproven_equivalence_are_refused() {
        let mut follows = cross_about_request();
        follows["connect_to"][0]["rel"] = json!("follows");
        follows["connect_to"][0]["class"] = json!("procedural");
        let error =
            build_write_plan_with_root(&follows, false).expect_err("no other relation crosses");
        assert!(
            error.contains("only with `same_event_as` or `same_entity_as`"),
            "{error}"
        );

        let mut unproven = cross_about_request();
        unproven["read_context"] = json!({"inspected_refs": ["incident:platform:outcome:freeze"]});
        let error = build_write_plan_with_root(&unproven, false)
            .expect_err("an inspection is not a proposal");
        assert!(error.contains("read_context.relate_proposals"), "{error}");

        let mut foreign_pair = cross_about_request();
        foreign_pair["read_context"]["relate_proposals"][0]["from"] =
            json!("incident:other:entry:x");
        let error = build_write_plan_with_root(&foreign_pair, false)
            .expect_err("one ref of the proposal must be this about's");
        assert!(error.contains("read_context.relate_proposals"), "{error}");
    }
}
