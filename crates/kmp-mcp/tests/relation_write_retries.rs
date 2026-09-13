//! A logical attachment stays independent from mutable endpoint contents.

#[path = "support/relation_write_fixture.rs"]
pub mod fixture;
use fixture::*;
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

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
async fn an_exact_retry_survives_endpoint_replacement_and_relabelling() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let (alias, notice) = sources(&server).await;
    let request = junco_link(&alias, &notice);
    let original = write(&server, request.clone()).await;
    assert_eq!(original["status"], "committed");
    correct_alias(&server, &alias).await;
    call(
        &server,
        "kmp_relabel",
        json!({
            "about":ABOUT,"ref":notice,"actor":"agent:other","observed_at":LATER,
            "why":"The notice is now indexed by its owner.","add":{"owner":["nora"]},
            "idempotency_key":"notice-owner-label"
        }),
    )
    .await;
    let current = [
        inspected(&server, &alias).await,
        inspected(&server, &notice).await,
    ];
    let retry = structured(&server, "kmp_write_memory", request).await;
    assert_eq!(
        retry["structuredContent"]["status"], "replayed",
        "An identical logical request should replay after an endpoint update: {retry}"
    );
    assert_eq!(retry["structuredContent"]["receipt"], original["receipt"]);
    assert_eq!(retry["structuredContent"]["clocks"], original["clocks"]);
    assert_eq!(inspected(&server, &alias).await["raw"], current[0]["raw"]);
    assert_eq!(inspected(&server, &notice).await["raw"], current[1]["raw"]);
}

#[tokio::test]
async fn a_fallback_link_preserves_a_concurrent_source_update() {
    let dir = scratch();
    let inner = std::sync::Arc::new(
        kmp_mcp::EmbeddedKernelMcpBackend::open(dir.path()).expect("embedded backend"),
    );
    let original = KernelMcpServer::with_backend(inner.clone());
    let (alias, notice) = sources(&original).await;
    let before = inspected(&original, &alias).await;
    let competing = json!({
        "about":ABOUT,"idempotency_key":"review-interleaved-correction",
        "memory":{"dimensions":[],"entries":[{
            "id":alias,"kind":"observation","text":CORRECTED,
            "coordinates":before["raw"][0]["coordinates"],"metadata":before["object"]["metadata"]
        }],"relations":[],"evidence":[]},
        "provenance":{"source_kind":"agent","source_agent":"agent:other","observed_at":LATER}
    });
    let server = KernelMcpServer::with_backend(InterleavingBackend {
        inner,
        competing_write: std::sync::Mutex::new(Some(competing)),
    });
    let mut request = junco_link(&alias, &notice);
    request["idempotency_key"] = json!("review-interleaved-link");
    request["relations"][0]["rel"] = json!("follows");
    let result = write(&server, request).await;
    assert_eq!(result["status"], "committed", "{result}");
    let after = inspected(&server, &alias).await;
    assert_eq!(
        after["object"]["text"], CORRECTED,
        "A relation-only write must not overwrite the source update that committed after its pre-read: {after}"
    );
}

struct InterleavingBackend {
    inner: std::sync::Arc<kmp_mcp::EmbeddedKernelMcpBackend>,
    competing_write: std::sync::Mutex<Option<Value>>,
}

impl kmp_mcp::KernelMcpToolBackend for InterleavingBackend {
    fn backend_name(&self) -> &'static str {
        "embedded"
    }
    fn call_tool<'a>(
        &'a self,
        name: &'a str,
        arguments: &'a Value,
    ) -> kmp_mcp::KernelMcpToolFuture<'a> {
        Box::pin(async move {
            if name == "kmp_ingest" {
                let competing = self.competing_write.lock().expect("competing write").take();
                if let Some(competing) = competing {
                    let updated = self.inner.call_tool("kmp_ingest", &competing).await?;
                    assert_ne!(
                        updated["isError"], true,
                        "Competing update must commit: {updated}"
                    );
                    let check = self
                        .inner
                        .call_tool(
                            "kmp_inspect",
                            &json!({
                                "about":ABOUT,"ref":competing["memory"]["entries"][0]["id"],
                                "include":{"details":true,"raw":true}
                            }),
                        )
                        .await?;
                    let check = check.get("structuredContent").unwrap_or(&check);
                    assert_eq!(
                        check["object"]["text"], CORRECTED,
                        "The corrected source was stored before the attachment: {check}"
                    );
                }
            }
            self.inner.call_tool(name, arguments).await
        })
    }
}
