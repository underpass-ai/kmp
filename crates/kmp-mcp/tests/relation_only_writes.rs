//! Linking two memories that already exist, without rewriting either (#663).
//!
//! The Junco check linked an alias entry observed on 1 September to a
//! responsibility notice observed on 8 September. The only way to declare
//! that link was to resubmit the alias as a `memories` record, so the alias
//! lost its prose, its evidence and its date to a paraphrase mentioning both
//! sources — and a read at 1 September afterwards returned text about an
//! event that had not happened yet. These tests write the same link through
//! `relations` and hold both sources to the byte.

use serde_json::{Value, json};

use kmp_mcp::KernelMcpServer;

const ABOUT: &str = "project:junco";
const EARLY: &str = "2026-09-01T09:00:00Z";
const LATE: &str = "2026-09-08T11:00:00Z";
const LATER: &str = "2026-09-10T08:00:00Z";
const ALIAS: &str = "J01 registers Nora as the operational alias of Leonor Alba.";
const NOTICE: &str = "J03 assigns responsibility for the shared store to Nora.";

fn scratch() -> tempfile::TempDir {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
    std::fs::create_dir_all(&root).expect("scratch root");
    tempfile::tempdir_in(root).expect("isolated store")
}

async fn structured(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":tool,"arguments":arguments}});
    let wire = server
        .handle_json_line(&request.to_string())
        .await
        .expect("response");
    serde_json::from_str::<Value>(&wire).expect("JSON")["result"].clone()
}

async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let result = structured(server, tool, arguments).await;
    assert_eq!(result["isError"], false, "{result}");
    result["structuredContent"].clone()
}

async fn refused(server: &KernelMcpServer, arguments: Value) -> Value {
    let result = structured(server, "kmp_write_memory", arguments).await;
    assert_eq!(result["isError"], true, "{result}");
    result["structuredContent"].clone()
}

/// A rich link is reviewed before it commits, in every shape that declares
/// one. Resuming the returned continuation is what an agent does.
async fn write(server: &KernelMcpServer, arguments: Value) -> Value {
    let result = call(server, "kmp_write_memory", arguments).await;
    if result["status"] != "needs_review" {
        return result;
    }
    let action = &result["next_actions"][0];
    call(
        server,
        action["tool"].as_str().expect("verb"),
        action["arguments"].clone(),
    )
    .await
}

async fn seed(server: &KernelMcpServer, key: &str, id: &str, text: &str, at: &str) -> String {
    let result = write(
        server,
        json!({"about":ABOUT,"actor":"agent:sol","idempotency_key":key,"observed_at":at,
            "labels":{"task":["junco"]},
            "memories":[{"id":id,"kind":"observation","summary":text,
                "evidence":format!("Junco register, entry {id}.")}]}),
    )
    .await;
    assert_eq!(result["status"], "committed", "{result}");
    result["local_refs"][id]
        .as_str()
        .expect("canonical ref")
        .to_owned()
}

async fn sources(server: &KernelMcpServer) -> (String, String) {
    let alias = seed(server, "junco-j01", "alias", ALIAS, EARLY).await;
    let notice = seed(server, "junco-j03", "notice", NOTICE, LATE).await;
    (alias, notice)
}

async fn inspected(server: &KernelMcpServer, reference: &str) -> Value {
    call(
        server,
        "kmp_inspect",
        json!({"about":ABOUT,"ref":reference,"budget":{"max_bytes":200000},
            "include":{"details":true,"raw":true,"incoming":true,"outgoing":true}}),
    )
    .await
}

fn link(from: &str, to: &str, rel: &str, why: &str, evidence: &str) -> Value {
    json!({"from":from,"to":to,"rel":rel,"why":why,"evidence":evidence})
}

fn packet(from: &str, to: &str, key: &str, at: &str, rel: &str, why: &str, proof: &str) -> Value {
    json!({"about":ABOUT,"actor":"agent:sol","idempotency_key":key,"observed_at":at,
        "read_context":{"inspected_refs":[from,to]},
        "relations":[link(from, to, rel, why, proof)]})
}

fn junco_link(from: &str, to: &str) -> Value {
    packet(
        from,
        to,
        "junco-link-v1",
        LATE,
        "supports",
        "The alias register is what identifies the person the notice makes responsible.",
        "J01 registers the alias Nora; J03 names Nora as responsible.",
    )
}

#[tokio::test]
async fn a_declared_link_leaves_both_sources_exactly_as_they_were() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let (alias, notice) = sources(&server).await;
    let before = [
        inspected(&server, &alias).await,
        inspected(&server, &notice).await,
    ];

    let written = write(&server, junco_link(&alias, &notice)).await;
    assert_eq!(written["status"], "committed", "{written}");

    for (reference, before) in [(&alias, &before[0]), (&notice, &before[1])] {
        let after = inspected(&server, reference).await;
        assert_eq!(after["object"]["text"], before["object"]["text"], "text");
        assert_eq!(after["object"]["kind"], before["object"]["kind"], "kind");
        assert_eq!(
            after["object"]["metadata"], before["object"]["metadata"],
            "metadata"
        );
        assert_eq!(
            after["raw"][0]["coordinates"],
            before["raw"][0]["coordinates"]
        );
        // The evidence the source already carried is still there, unchanged,
        // beside whatever this write added.
        for earlier in before["evidence"].as_array().expect("stored evidence") {
            assert!(
                after["evidence"]
                    .as_array()
                    .expect("evidence")
                    .iter()
                    .any(|item| item["id"] == earlier["id"] && item["text"] == earlier["text"]),
                "`{reference}` lost evidence {earlier}: {after}"
            );
        }
    }
}

#[tokio::test]
async fn the_receipt_separates_the_created_link_from_the_untouched_sources() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let (alias, notice) = sources(&server).await;

    let written = write(&server, junco_link(&alias, &notice)).await;
    let attachment = &written["attachment"];
    assert_eq!(attachment["created"]["relations"][0]["from"], alias);
    assert_eq!(attachment["created"]["relations"][0]["rel"], "supports");
    assert_eq!(attachment["created"]["relations"][0]["to"], notice);
    assert_eq!(attachment["created"]["relations"][0]["observed_at"], LATE);
    assert_eq!(
        attachment["created"]["evidence"][0],
        attachment["created"]["relations"][0]["evidence_ref"]
    );
    assert_eq!(
        attachment["unchanged_sources"],
        json!([alias.clone(), notice.clone()]),
        "{written}"
    );
    assert_eq!(written["coverage"]["memories"], 0);
    assert_eq!(written["coverage"]["relations"], 1);
    assert_eq!(written["generated_refs"], json!([]));
    assert!(written.get("replacement").is_none(), "{written}");
    assert!(
        written["summary"]
            .as_str()
            .expect("summary")
            .starts_with("Attached 1 relation"),
        "{written}"
    );
}

#[tokio::test]
async fn replacing_a_memory_at_a_supplied_ref_says_so_instead_of_reading_as_an_attachment() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let (alias, _) = sources(&server).await;

    let written = write(
        &server,
        json!({"about":ABOUT,"actor":"agent:sol","idempotency_key":"junco-j01-rewrite",
            "observed_at":LATER,"labels":{"task":["junco"]},
            "memories":[{"id":"alias","ref":alias,"kind":"observation",
                "summary":"J01 registers Nora as the operational alias of Leonor Alba (corrected).",
                "evidence":"Junco register, corrected entry J01."}]}),
    )
    .await;
    assert_eq!(written["status"], "committed", "{written}");
    let replaced = &written["replacement"]["memories"][0];
    assert_eq!(replaced["ref"], alias);
    assert_eq!(replaced["observed_at"], LATER);
    assert_eq!(
        replaced["evidence"][0],
        format!("evidence:{alias}:current"),
        "{written}"
    );
    assert!(written.get("attachment").is_none(), "{written}");
}

#[tokio::test]
async fn the_link_is_dated_when_it_was_asserted_and_not_when_its_sources_were() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let (alias, notice) = sources(&server).await;
    write(&server, junco_link(&alias, &notice)).await;

    // Before the declaration neither the link nor its evidence exists, and
    // the alias still reads exactly as it did on 1 September — which is what
    // the resubmitted-source route could not promise.
    let before = at(&server, "2026-09-08T10:59:59Z", &alias).await;
    assert_eq!(before["entries"][0]["text"], ALIAS, "{before}");
    assert!(!links(&before, &alias, &notice), "{before}");
    assert!(
        !cites(&before, "J03 names Nora as responsible"),
        "the link's evidence must not be readable before it was declared: {before}"
    );

    let after = at(&server, LATE, &alias).await;
    assert_eq!(
        after["entries"][0]["text"], ALIAS,
        "still the stored source"
    );
    assert!(links(&after, &alias, &notice), "{after}");
    assert!(cites(&after, "J03 names Nora as responsible"), "{after}");
}

/// `kmp_goto` at one instant, with the link and its proof requested.
async fn at(server: &KernelMcpServer, cut: &str, reference: &str) -> Value {
    call(
        server,
        "kmp_goto",
        json!({"about":ABOUT,"axis":"observed","at":{"time":cut},"refs":[reference],
            "budget":{"max_bytes":200000},"include":{"evidence":true,"relations":true}}),
    )
    .await
}

fn links(result: &Value, from: &str, to: &str) -> bool {
    result["proof"]["matched_relations"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(result["proof"]["path"].as_array().into_iter().flatten())
        .any(|relation| relation["from"] == from && relation["to"] == to)
}

fn cites(result: &Value, fragment: &str) -> bool {
    result["proof"]["evidence"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|item| {
            item["text"]
                .as_str()
                .is_some_and(|text| text.contains(fragment))
        })
}

#[tokio::test]
async fn an_exact_retry_replays_and_the_same_key_with_a_different_link_is_refused() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let (alias, notice) = sources(&server).await;

    let first = write(&server, junco_link(&alias, &notice)).await;
    assert_eq!(first["status"], "committed", "{first}");
    let retry = write(&server, junco_link(&alias, &notice)).await;
    assert_eq!(retry["status"], "replayed", "{retry}");
    assert_eq!(
        retry["attachment"]["created"]["evidence"], first["attachment"]["created"]["evidence"],
        "an exact retry keeps its evidence identity"
    );

    let mut changed = junco_link(&alias, &notice);
    changed["relations"][0]["why"] = json!("A different rationale under the same key.");
    let refusal = refused(&server, changed).await;
    assert!(
        refusal["error"]["message"]
            .as_str()
            .expect("message")
            .contains("already accepted with different content"),
        "{refusal}"
    );
}

#[tokio::test]
async fn a_second_link_to_the_same_pair_keeps_the_first_link_and_its_evidence() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let (alias, notice) = sources(&server).await;
    let first = write(&server, junco_link(&alias, &notice)).await;

    let second = write(
        &server,
        packet(
            &alias,
            &notice,
            "junco-link-v2",
            LATER,
            "contributes_to",
            "The alias register is one of the inputs to the responsibility assignment.",
            "J03 cites the alias register among its inputs.",
        ),
    )
    .await;
    assert_eq!(second["status"], "committed", "{second}");

    let first_evidence = first["attachment"]["created"]["evidence"][0].clone();
    let second_evidence = second["attachment"]["created"]["evidence"][0].clone();
    assert_ne!(
        first_evidence, second_evidence,
        "a later link must not take the earlier link's evidence id"
    );
    assert_eq!(
        second["attachment"]["created"]["relations"][0]["observed_at"],
        LATER
    );

    let stored = inspected(&server, &alias).await;
    for expected in [&first_evidence, &second_evidence] {
        let expected = expected.as_str().expect("evidence ref");
        assert!(
            stored["evidence"]
                .as_array()
                .expect("evidence")
                .iter()
                .any(|item| item["id"].as_str().is_some_and(|id| id.ends_with(expected))),
            "{expected} is missing from {stored}"
        );
    }
    assert_eq!(stored["object"]["text"], ALIAS, "still the stored source");
}

#[tokio::test]
async fn the_three_shapes_are_exclusive_and_nothing_that_moves_a_source_may_accompany_a_link() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let (alias, notice) = sources(&server).await;

    let mut both = junco_link(&alias, &notice);
    both["memories"] = json!([{"id":"x","kind":"observation","summary":"y","evidence":"z"}]);
    let refusal = refused(&server, both).await;
    assert_eq!(
        refusal["feedback"][0]["code"], "WRITE_OPERATION_REQUIRED",
        "{refusal}"
    );

    for field in ["labels", "occurred_at", "valid_from", "valid_until", "rank"] {
        let mut moving = junco_link(&alias, &notice);
        moving[field] = if field == "labels" {
            json!({"task":["junco"]})
        } else if field == "rank" {
            json!(2)
        } else {
            json!(LATE)
        };
        let refusal = refused(&server, moving).await;
        assert_eq!(
            refusal["feedback"][0]["code"], "PRESERVED_FIELD",
            "`{field}` must not accompany relations: {refusal}"
        );
    }

    let mut missing = junco_link(&alias, &notice);
    missing["relations"][0]["to"] = json!(format!("{ABOUT}:entry:observation:not-there"));
    let refusal = refused(&server, missing).await;
    assert!(
        refusal["error"]["message"]
            .as_str()
            .expect("message")
            .contains("relations could not read"),
        "{refusal}"
    );

    let mut foreign = junco_link(&alias, &notice);
    foreign["relations"][0]["to"] = json!("project:other:entry:observation:elsewhere");
    let refusal = refused(&server, foreign).await;
    assert_eq!(
        refusal["feedback"][0]["code"], "CROSS_ABOUT_RELATION",
        "{refusal}"
    );

    let mut proofless = junco_link(&alias, &notice);
    proofless["relations"][0]
        .as_object_mut()
        .expect("link")
        .remove("evidence");
    let refusal = refused(&server, proofless).await;
    assert_eq!(
        refusal["feedback"][0]["code"], "RELATION_PROOF_REQUIRED",
        "{refusal}"
    );
    assert_eq!(
        refusal["feedback"][0]["field"], "relations[0].evidence",
        "{refusal}"
    );
}

/// The same packet, compiled by the same writer, committed through the real
/// gRPC memory service instead of the embedded backend. The link, its clock
/// and its evidence have to survive the proto mapping intact.
#[tokio::test]
async fn a_link_declared_over_grpc_commits_exactly_what_the_embedded_path_commits() {
    let embedded_dir = scratch();
    let embedded = KernelMcpServer::embedded(embedded_dir.path()).expect("embedded");
    let (alias, notice) = sources(&embedded).await;
    let direct = write(&embedded, junco_link(&alias, &notice)).await;

    let remote_dir = scratch();
    let kernel = kmp_embedded::EmbeddedKernel::open(remote_dir.path()).expect("kernel");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("port");
    let endpoint = format!("http://{}", listener.local_addr().expect("address"));
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(
                kmp_proto::v1beta1::kernel_memory_service_server::KernelMemoryServiceServer::new(
                    kmp_transport_grpc::MemoryGrpcService::new(kernel.service()),
                ),
            )
            .serve_with_incoming_shutdown(
                tokio_stream::wrappers::TcpListenerStream::new(listener),
                async {
                    let _ = stopped.await;
                },
            ),
    );
    let server = KernelMcpServer::grpc(endpoint);
    let (remote_alias, remote_notice) = sources(&server).await;
    assert_eq!(
        remote_alias, alias,
        "refs are deterministic across backends"
    );
    let remote = write(&server, junco_link(&remote_alias, &remote_notice)).await;

    assert_eq!(remote["status"], direct["status"]);
    assert_eq!(remote["attachment"], direct["attachment"]);
    assert_eq!(remote["coverage"], direct["coverage"]);
    assert_eq!(remote["relations"], direct["relations"]);
    let stored = inspected(&server, &remote_alias).await;
    assert_eq!(stored["object"]["text"], ALIAS, "{stored}");

    let _ = stop.send(());
    let _ = serving.await;
}

#[tokio::test]
async fn a_rich_link_is_reviewed_before_it_commits_in_this_shape_too() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let (alias, notice) = sources(&server).await;

    let pending = call(&server, "kmp_write_memory", junco_link(&alias, &notice)).await;
    assert_eq!(pending["status"], "needs_review", "{pending}");
    assert_eq!(pending["accepted"], false, "nothing is written yet");
    assert_eq!(pending["relations"][0]["from"], alias, "{pending}");
    let resume = &pending["next_actions"][0];
    assert_eq!(resume["tool"], "kmp_write_memory");
    assert_eq!(
        resume["arguments"]["review_token"], pending["neighborhood"]["token"],
        "the continuation carries the token it was served"
    );
    // The alias is untouched while the review is outstanding.
    assert_eq!(inspected(&server, &alias).await["object"]["text"], ALIAS);

    let committed = call(&server, "kmp_write_memory", resume["arguments"].clone()).await;
    assert_eq!(committed["status"], "committed", "{committed}");
}
