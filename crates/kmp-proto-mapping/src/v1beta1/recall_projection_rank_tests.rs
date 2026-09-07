use super::*;

fn ranked_response() -> AskResponse {
    AskResponse {
        answer: "UNKNOWN".into(),
        proof: Some(kmp_proto::v1beta1::Proof {
            confidence: MemoryConfidence::Low as i32,
            evidence: ["z-best", "m-middle", "a-last"]
                .into_iter()
                .map(|id| MemoryEvidence {
                    id: id.into(),
                    supports: vec![format!("project:kmp:observation:{id}")],
                    text: format!("Stored text for {id}."),
                    source: "stored source".into(),
                    metadata: [("reached_by".into(), "association".into())]
                        .into_iter()
                        .collect(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn request(cursor: Option<String>) -> AskRequest {
    AskRequest {
        about: "project:kmp".into(),
        question: "Which evidence is relevant?".into(),
        budget: Some(kmp_proto::v1beta1::MemoryBudget {
            max_bytes: 4000,
            detail: MemoryDetailLevel::Full as i32,
            ..Default::default()
        }),
        page: Some(kmp_proto::v1beta1::PageRequest {
            entries: 1,
            cursor: cursor.unwrap_or_default(),
        }),
        ..Default::default()
    }
}

#[test]
fn ask_pages_preserve_retrieval_rank_instead_of_sorting_evidence_ids() {
    let response = ranked_response();
    let mut cursor = None;
    let mut observed = Vec::new();
    loop {
        let projected = project_ask_response(response.clone(), &request(cursor)).expect("page");
        let rendered = ask_value(&projected);
        assert!(serde_json::to_vec(&rendered).expect("bytes").len() <= 4000);
        assert_eq!(projected.answer, "UNKNOWN");
        assert!(projected.because.is_empty());
        observed.extend(
            projected
                .proof
                .expect("proof")
                .evidence
                .into_iter()
                .map(|e| e.id),
        );
        let page = projected
            .projection
            .expect("projection")
            .page
            .expect("page");
        if !page.has_more {
            break;
        }
        cursor = page.next_cursor;
        assert!(observed.len() < 4, "pagination must terminate");
    }
    assert_eq!(observed, ["z-best", "m-middle", "a-last"]);
}

#[test]
fn ask_cursor_binds_both_evidence_rank_and_content() {
    let response = ranked_response();
    let first = project_ask_response(response.clone(), &request(None)).expect("first");
    let cursor = first
        .projection
        .expect("projection")
        .page
        .expect("page")
        .next_cursor
        .expect("cursor");
    for change_rank in [true, false] {
        let mut changed = response.clone();
        let evidence = &mut changed.proof.as_mut().expect("proof").evidence;
        if change_rank {
            evidence.swap(0, 2);
        } else {
            evidence[2].text = "Changed stored text.".into();
        }
        let error = project_ask_response(changed, &request(Some(cursor.clone())))
            .expect_err("changed rank or content must invalidate the cursor");
        assert!(error.to_string().contains("does not match"));
    }
}
