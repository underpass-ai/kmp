use std::sync::{Arc, Mutex};

use kmp_mcp::{KernelMcpGrpcTlsConfig, KernelMcpServer, KernelMcpToolBackend, KernelMcpToolFuture};
use serde_json::{Value, json};

#[test]
fn backend_selection_helper_uses_fixture_when_endpoint_is_absent() {
    let server = KernelMcpServer::from_optional_endpoint(None);
    assert_eq!(server.backend_name(), "fixture");
}

#[test]
fn backend_selection_uses_grpc_when_endpoint_is_present() {
    let server =
        KernelMcpServer::from_optional_endpoint(Some("http://127.0.0.1:50051".to_string()));
    assert_eq!(server.backend_name(), "grpc");
}

#[test]
fn backend_selection_reports_grpc_tls_mode() {
    let server = KernelMcpServer::grpc_with_tls(
        "https://kmp.underpassai.com",
        KernelMcpGrpcTlsConfig::server("/tmp/ca.crt", None),
    );

    assert_eq!(server.backend_name(), "grpc");
    assert_eq!(server.grpc_tls_mode_name(), "server");
}

#[test]
fn backend_selection_ignores_blank_endpoint() {
    let server = KernelMcpServer::from_optional_endpoint(Some("   ".to_string()));
    assert_eq!(server.backend_name(), "fixture");
}

#[tokio::test]
async fn initialize_declares_tools_capability() {
    let response = handle(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {}
    }))
    .await;

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 1);
    assert_eq!(
        response["result"]["serverInfo"]["name"],
        "underpass-kmp-mcp"
    );
    assert_eq!(response["result"]["metadata"]["backend"], "fixture");
    assert!(response["result"]["capabilities"].get("tools").is_some());
}

#[tokio::test]
async fn malformed_json_returns_jsonrpc_parse_error() {
    let server = KernelMcpServer::fixture();
    let response = server
        .handle_json_line("{not-json")
        .await
        .expect("malformed JSON should produce an error response");
    let response = serde_json::from_str::<Value>(&response).expect("response should be JSON");

    assert_eq!(response["error"]["code"], -32700);
    assert!(
        response["error"]["message"]
            .as_str()
            .expect("error should include message")
            .contains("invalid JSON-RPC message")
    );
}

#[tokio::test]
async fn missing_method_returns_jsonrpc_request_error() {
    let response = handle(json!({
        "jsonrpc": "2.0",
        "id": 21,
        "params": {}
    }))
    .await;

    assert_eq!(response["error"]["code"], -32600);
    assert_eq!(response["error"]["message"], "missing JSON-RPC method");
}

#[tokio::test]
async fn unsupported_method_returns_jsonrpc_method_error() {
    let response = handle(json!({
        "jsonrpc": "2.0",
        "id": 22,
        "method": "resources/list",
        "params": {}
    }))
    .await;

    assert_eq!(response["error"]["code"], -32601);
    assert!(
        response["error"]["message"]
            .as_str()
            .expect("error should include message")
            .contains("unsupported JSON-RPC method")
    );
}

#[tokio::test]
async fn negotiated_mcp_app_is_discoverable_and_keeps_chunks_out_of_text_context() {
    let server = KernelMcpServer::fixture();
    let invoke = |request: Value| {
        let server = &server;
        async move {
            let response = server
                .handle_json_line(&request.to_string())
                .await
                .expect("request response");
            serde_json::from_str::<Value>(&response).expect("JSON-RPC response")
        }
    };
    let initialized = invoke(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"capabilities": {"extensions": {
            "io.modelcontextprotocol/ui": {"mimeTypes": ["text/html;profile=mcp-app"]}
        }}}
    }))
    .await;
    assert!(initialized["result"]["capabilities"]["resources"].is_object());

    let tools = invoke(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).await;
    let definitions = tools["result"]["tools"].as_array().expect("tools");
    let open = definitions
        .iter()
        .find(|tool| tool["name"] == "kmp_view_open")
        .expect("view opener");
    assert_eq!(
        open["_meta"]["ui"]["resourceUri"],
        "ui://kmp/chronoloom.html"
    );
    let projection = definitions
        .iter()
        .find(|tool| tool["name"] == "kmp_view_read_projection")
        .expect("the negotiated host must discover the app-only data tool");
    assert_eq!(projection["_meta"]["ui"]["visibility"], json!(["app"]));
    let undo = definitions
        .iter()
        .find(|tool| tool["name"] == "kmp_view_undo")
        .expect("the app must retain the loopback undo semantics");
    assert_eq!(undo["_meta"]["ui"]["visibility"], json!(["app"]));

    let resources = invoke(json!({"jsonrpc": "2.0", "id": 3, "method": "resources/list"})).await;
    assert_eq!(
        resources["result"]["resources"][0]["uri"],
        "ui://kmp/chronoloom.html"
    );
    let resource = invoke(json!({
        "jsonrpc": "2.0", "id": 4, "method": "resources/read",
        "params": {"uri": "ui://kmp/chronoloom.html"}
    }))
    .await;
    assert_eq!(
        resource["result"]["contents"][0]["mimeType"],
        "text/html;profile=mcp-app"
    );
    assert!(
        resource["result"]["contents"][0]["text"]
            .as_str()
            .expect("HTML")
            .contains("kmp_view_read_projection")
    );

    let chunk = invoke(json!({
        "jsonrpc": "2.0", "id": 5, "method": "tools/call",
        "params": {"name": "kmp_view_read_projection", "arguments": {
            "about": "project:kmp",
            "from": "2026-08-01T00:00:00Z",
            "to": "2026-09-01T00:00:00Z",
            "lod": "moment"
        }}
    }))
    .await;
    let receipt = chunk["result"]["content"][0]["text"]
        .as_str()
        .expect("text receipt");
    assert!(
        receipt.len() < 160,
        "model-context receipt grew into a data payload"
    );
    assert_eq!(chunk["result"]["_meta"]["kmp/modelContext"], "receipt-only");
    assert!(chunk["result"]["structuredContent"]["entries"].is_array());
}

#[tokio::test]
async fn tools_list_exposes_read_only_kmp_tools() {
    let response = handle(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    }))
    .await;

    let tool_names = response["result"]["tools"]
        .as_array()
        .expect("tools should be an array")
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool should have a name"))
        .collect::<Vec<_>>();

    assert_eq!(
        tool_names,
        vec![
            "kmp_ingest",
            "kmp_write_memory",
            "kmp_wake",
            "kmp_ask",
            "kmp_goto",
            "kmp_near",
            "kmp_rewind",
            "kmp_forward",
            "kmp_trace",
            "kmp_inspect",
            // The view half: an agent moves what a person is looking at by
            // declaring intent, and none of these three can write memory.
            "kmp_view_open",
            "kmp_view_apply_intent",
            "kmp_view_get_state"
        ]
    );

    let writers = ["kmp_ingest", "kmp_write_memory"];
    for tool in response["result"]["tools"].as_array().expect("tools") {
        let name = tool["name"].as_str().expect("name");
        if !name.starts_with("kmp_view_") {
            continue;
        }
        let schema = tool["inputSchema"]["properties"]
            .as_object()
            .expect("view tools take an object");
        for writer in writers {
            assert!(
                !schema.contains_key(writer),
                "`{name}` must not reach {writer}: a visual action never writes memory"
            );
        }
        assert!(
            tool["description"].as_str().expect("description").len() > 40,
            "`{name}` has to say what it does to the view"
        );
    }
}

#[tokio::test]
async fn former_kernel_names_are_accepted_but_not_advertised() {
    let calls = [
        ("kernel_ingest", sample_ingest_arguments()),
        ("kernel_write_memory", sample_write_arguments(true)),
        ("kernel_wake", json!({"about": "question:830ce83f"})),
        (
            "kernel_ask",
            json!({
                "about": "question:830ce83f",
                "question": "Where did Rachel move?"
            }),
        ),
        (
            "kernel_goto",
            json!({
                "about": "question:830ce83f",
                "at": {"ref": "claim:rachel-austin"}
            }),
        ),
        (
            "kernel_near",
            json!({
                "about": "question:830ce83f",
                "around": {"ref": "claim:rachel-austin"}
            }),
        ),
        (
            "kernel_rewind",
            json!({
                "about": "question:830ce83f",
                "from": {"ref": "claim:rachel-austin"}
            }),
        ),
        (
            "kernel_forward",
            json!({
                "about": "question:830ce83f",
                "from": {"ref": "claim:rachel-austin"}
            }),
        ),
        (
            "kernel_trace",
            json!({
                "from": "claim:rachel-austin",
                "to": "claim:rachel-denver"
            }),
        ),
        ("kernel_inspect", json!({"ref": "claim:rachel-austin"})),
    ];

    for (id, (name, arguments)) in calls.into_iter().enumerate() {
        let response = handle(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": name, "arguments": arguments}
        }))
        .await;

        assert_eq!(response["result"]["isError"], false, "alias {name}");
    }
}

#[tokio::test]
async fn fixture_tools_cover_ingest_wake_trace_and_inspect() {
    let ingest = handle(json!({
        "jsonrpc": "2.0",
        "id": 23,
        "method": "tools/call",
        "params": {
            "name": "kmp_ingest",
            "arguments": sample_ingest_arguments()
        }
    }))
    .await;
    assert_eq!(ingest["result"]["isError"], false);
    assert_eq!(
        ingest["result"]["structuredContent"]["memory"]["memory_id"],
        "memory:830ce83f:1"
    );

    let wake = handle(json!({
        "jsonrpc": "2.0",
        "id": 24,
        "method": "tools/call",
        "params": {
            "name": "kmp_wake",
            "arguments": {
                "about": "memory:kernel-memory-protocol"
            }
        }
    }))
    .await;
    assert_eq!(wake["result"]["isError"], false);
    assert!(wake["result"]["structuredContent"]["wake"].is_object());

    let trace = handle(json!({
        "jsonrpc": "2.0",
        "id": 25,
        "method": "tools/call",
        "params": {
            "name": "kmp_trace",
            "arguments": {
                "from": "claim:rachel-austin",
                "to": "claim:rachel-denver"
            }
        }
    }))
    .await;
    assert_eq!(trace["result"]["isError"], false);
    assert_eq!(
        trace["result"]["structuredContent"]["trace"][0]["rel"],
        "supersedes"
    );

    let near = handle(json!({
        "jsonrpc": "2.0",
        "id": 28,
        "method": "tools/call",
        "params": {
            "name": "kmp_near",
            "arguments": {
                "about": "question:830ce83f",
                "around": {
                    "time": "2026-04-12T15:03:00Z"
                }
            }
        }
    }))
    .await;
    assert_eq!(near["result"]["isError"], false);
    assert_eq!(
        near["result"]["structuredContent"]["temporal"]["direction"],
        "near"
    );

    let inspect = handle(json!({
        "jsonrpc": "2.0",
        "id": 26,
        "method": "tools/call",
        "params": {
            "name": "kmp_inspect",
            "arguments": {
                "ref": "claim:rachel-austin"
            }
        }
    }))
    .await;
    assert_eq!(inspect["result"]["isError"], false);
    assert_eq!(
        inspect["result"]["structuredContent"]["object"]["ref"],
        "claim:rachel-austin"
    );
}

#[tokio::test]
async fn kmp_ask_returns_fixture_backed_structured_content() {
    let response = handle(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "kmp_ask",
            "arguments": {
                "about": "question:830ce83f",
                "question": "Where did Rachel move after her recent relocation?",
                "answer_policy": "evidence_or_unknown"
            }
        }
    }))
    .await;

    assert_eq!(response["result"]["isError"], false);
    assert_eq!(
        response["result"]["structuredContent"]["answer"],
        "Memory answer supported by claim:rachel-austin [evidence:rachel-turn-2]; canonical text is in proof.evidence."
    );
    assert_eq!(
        response["result"]["structuredContent"]["proof"]["evidence"][0]["text"],
        "Later she corrected it: the move is to Austin."
    );
    assert_eq!(
        response["result"]["structuredContent"]["proof"]["confidence"],
        "high"
    );
}

#[tokio::test]
async fn kmp_ask_rejects_a_cursor_from_another_projection() {
    let response = handle(json!({
        "jsonrpc": "2.0",
        "id": 30,
        "method": "tools/call",
        "params": {
            "name": "kmp_ask",
            "arguments": {
                "about": "question:830ce83f",
                "question": "Where did Rachel move after her recent relocation?",
                "answer_policy": "evidence_or_unknown",
                "page": {
                    "cursor": "kmp1:1:not-a-cursor-for-this-selection"
                }
            }
        }
    }))
    .await;

    assert_eq!(response["result"]["isError"], true);
    assert!(
        response["result"]["content"][0]["text"]
            .as_str()
            .is_some_and(|message| message.contains("invalid page.cursor"))
    );
}

#[tokio::test]
async fn ingest_aliases_return_fixture_backed_structured_content() {
    for name in ["kernel_remember", "kernel_ingest_context"] {
        let response = handle(json!({
            "jsonrpc": "2.0",
            "id": 27,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": sample_ingest_arguments()
            }
        }))
        .await;

        assert_eq!(response["result"]["isError"], false);
        assert_eq!(
            response["result"]["structuredContent"]["memory"]["accepted"]["entries"],
            2
        );
    }
}

#[tokio::test]
async fn kmp_write_memory_dry_run_returns_canonical_ingest_preview() {
    let response = handle(json!({
        "jsonrpc": "2.0",
        "id": 33,
        "method": "tools/call",
        "params": {
            "name": "kmp_write_memory",
            "arguments": sample_write_arguments(true)
        }
    }))
    .await;

    let structured = &response["result"]["structuredContent"];
    let fixture_response = serde_json::from_str::<Value>(include_str!(
        "../../../api/examples/kernel/v1beta1/kmp/write-memory.response.json"
    ))
    .expect("write fixture response should be valid JSON");

    assert_eq!(response["result"]["isError"], false);
    assert_eq!(structured, &fixture_response);
    assert_eq!(structured["accepted"], false);
    assert_eq!(structured["dry_run"], true);
    assert_eq!(
        structured["relation_quality_metrics"]["relation_rich_count"],
        3
    );
    assert_eq!(
        structured["ingest_preview"]["about"],
        "incident:mobile-login"
    );
    assert_eq!(
        structured["ingest_preview"]["memory"]["relations"][0]["rel"],
        "chosen_because"
    );
    assert_eq!(structured["next_suggested_reads"][0]["tool"], "kmp_trace");
}

#[tokio::test]
async fn kmp_write_memory_commit_uses_canonical_ingest_backend_path() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let server = KernelMcpServer::with_backend(StubBackend {
        calls: Arc::clone(&calls),
        backend_name: "stub",
        grpc_tls_mode_name: "disabled",
        response: Ok(json!({
            "content": [],
            "structuredContent": {
                "summary": "Ingested via stub",
                "memory": {
                    "about": "incident:mobile-login",
                    "memory_id": "memory:incident:mobile-login:1",
                    "accepted": {
                        "entries": 2,
                        "relations": 3,
                        "evidence": 3
                    },
                    "read_after_write_ready": true
                },
                "warnings": []
            },
            "isError": false
        })),
    });

    let response = handle_with(
        &server,
        json!({
            "jsonrpc": "2.0",
            "id": 34,
            "method": "tools/call",
            "params": {
                "name": "kmp_write_memory",
                "arguments": sample_write_arguments(false)
            }
        }),
    )
    .await;

    assert_eq!(response["result"]["isError"], false);
    assert_eq!(response["result"]["structuredContent"]["accepted"], true);
    assert_eq!(
        response["result"]["structuredContent"]["ingest_result"]["memory"]["accepted"]["relations"],
        3
    );

    let calls = calls.lock().expect("stub calls should be available");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "kmp_ingest");
    assert_eq!(calls[0].1["about"], "incident:mobile-login");
    assert_eq!(calls[0].1["dry_run"], false);
    assert_eq!(
        calls[0].1["memory"]["relations"][0]["rel"],
        "chosen_because"
    );
}

/// A viewer nobody is told about is a viewer nobody opens — the whole reason
/// phase 04 exists. The link rides on the write because that is the first
/// moment there is anything to look at, and it rides once because a link
/// repeated on every write is a link nobody reads.
#[tokio::test]
async fn the_first_write_of_a_session_hands_over_the_viewer_link_and_the_second_does_not() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let server = KernelMcpServer::with_backend(StubBackend {
        calls: Arc::clone(&calls),
        backend_name: "stub",
        grpc_tls_mode_name: "disabled",
        response: Ok(stub_ingest_response()),
    })
    .serving_viewer_at("http://127.0.0.1:7317/");

    let first = handle_with(&server, write_request(51)).await;
    let viewer = &first["result"]["structuredContent"]["viewer"];
    assert_eq!(viewer["url"], "http://127.0.0.1:7317/");
    assert!(
        viewer["tell_the_user"]
            .as_str()
            .expect("the invitation is written for a human")
            .contains("http://127.0.0.1:7317/"),
        "the sentence must carry the link, not just the sibling field"
    );

    let second = handle_with(&server, write_request(52)).await;
    assert_eq!(second["result"]["structuredContent"]["accepted"], true);
    assert!(
        second["result"]["structuredContent"]
            .get("viewer")
            .is_none(),
        "the invitation is said once a session"
    );
}

/// Every other backend reaches a kernel this process does not host, so there
/// is no local viewer to point at and no link to invent.
#[tokio::test]
async fn a_session_without_a_viewer_never_mentions_one() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let server = KernelMcpServer::with_backend(StubBackend {
        calls: Arc::clone(&calls),
        backend_name: "stub",
        grpc_tls_mode_name: "disabled",
        response: Ok(stub_ingest_response()),
    });

    let response = handle_with(&server, write_request(53)).await;
    assert_eq!(response["result"]["structuredContent"]["accepted"], true);
    assert!(
        response["result"]["structuredContent"]
            .get("viewer")
            .is_none()
    );
}

fn write_request(id: u64) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {
            "name": "kmp_write_memory",
            "arguments": sample_write_arguments(false)
        }
    })
}

fn stub_ingest_response() -> Value {
    json!({
        "content": [],
        "structuredContent": {
            "summary": "Ingested via stub",
            "memory": {
                "about": "incident:mobile-login",
                "memory_id": "memory:incident:mobile-login:1",
                "accepted": {"entries": 2, "relations": 3, "evidence": 3},
                "read_after_write_ready": true
            },
            "warnings": []
        },
        "isError": false
    })
}

/// Every tool declares `additionalProperties: false`. Until it was enforced,
/// a misspelled or misplaced argument was accepted, dropped, and answered with
/// a well-formed success built from defaults — so the agent read the result as
/// proof its arguments were understood and made the same call again.
#[tokio::test]
async fn an_argument_a_tool_does_not_have_is_refused_on_every_tool() {
    for name in kmp_mcp::kmp_mcp_tool_names() {
        let server = KernelMcpServer::fixture();
        let response = handle_with(
            &server,
            json!({
                "jsonrpc": "2.0",
                "id": 90,
                "method": "tools/call",
                "params": {
                    "name": name,
                    // Only the bogus key: the boundary runs before any
                    // required-argument check, so an unknown key is refused
                    // whether or not the rest of the call was well formed.
                    "arguments": {"definitely_not_an_argument": true}
                }
            }),
        )
        .await;

        let structured = &response["result"]["structuredContent"];
        assert_eq!(
            response["result"]["isError"], true,
            "`{name}` accepted an argument it does not have"
        );
        assert_eq!(
            structured["error"]["code"], "invalid_argument",
            "on `{name}`"
        );
        assert!(
            structured["error"]["message"]
                .as_str()
                .expect("a message")
                .contains("definitely_not_an_argument"),
            "the error has to name the key, on `{name}`: {structured}"
        );
    }
}

/// The nested schemas declare it too, and a `budget` key one level too deep is
/// the shape that looks most like it worked.
#[tokio::test]
async fn an_unknown_key_inside_budget_is_refused_with_its_path() {
    let server = KernelMcpServer::fixture();
    let response = handle_with(
        &server,
        json!({
            "jsonrpc": "2.0",
            "id": 91,
            "method": "tools/call",
            "params": {
                "name": "kmp_wake",
                "arguments": {"about": "question:x", "budget": {"max_bytes": 4000, "tokns": 900}}
            }
        }),
    )
    .await;

    let message = response["result"]["structuredContent"]["error"]["message"]
        .as_str()
        .expect("a message")
        .to_string();
    assert_eq!(response["result"]["isError"], true);
    assert!(message.contains("tokns"), "{message}");
    assert!(
        message.contains("kmp_wake.budget"),
        "say where it was, not just what it was: {message}"
    );
}

/// `prefer` was rejected by name in the Ask request builder — a branch only
/// reachable because the declared strictness was not applied. The boundary now
/// refuses it as one unknown key among all of them.
#[tokio::test]
async fn prefer_is_refused_by_the_schema_rather_than_by_a_special_case() {
    let server = KernelMcpServer::fixture();
    let response = handle_with(
        &server,
        json!({
            "jsonrpc": "2.0",
            "id": 92,
            "method": "tools/call",
            "params": {
                "name": "kmp_ask",
                "arguments": {"about": "question:x", "question": "why?", "prefer": {"time": "latest"}}
            }
        }),
    )
    .await;

    assert_eq!(response["result"]["isError"], true);
    assert_eq!(
        response["result"]["structuredContent"]["error"]["code"],
        "invalid_argument"
    );
    assert!(
        response["result"]["structuredContent"]["error"]["message"]
            .as_str()
            .expect("a message")
            .contains("prefer")
    );
}

#[tokio::test]
async fn first_strict_memory_can_form_an_about_root_but_later_writes_need_a_link() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let server = KernelMcpServer::embedded(data_dir.path()).expect("embedded server");
    let arguments = json!({
        "about": "project:new-root",
        "intent": "record_decision",
        "actor": "test-agent",
        "observed_at": "2026-08-17T10:00:00Z",
        "scope": {"process": "process:test"},
        "current": {
            "kind": "decision",
            "summary": "The first durable memory for this project",
            "evidence": "The project has no prior KMP entries."
        },
        "options": {"strict": true}
    });
    let request = |id| {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": "kmp_write_memory", "arguments": arguments.clone()}
        })
    };

    let first = handle_with(&server, request(70)).await;
    assert_eq!(first["result"]["isError"], false, "{first}");
    assert_eq!(first["result"]["structuredContent"]["accepted"], true);

    let second = handle_with(&server, request(71)).await;
    assert_eq!(second["result"]["isError"], true, "{second}");
    assert!(
        second["result"]["content"][0]["text"]
            .as_str()
            .expect("validation message")
            .contains("once the about exists")
    );
}

#[tokio::test]
async fn invalid_ingest_arguments_return_tool_error() {
    let response = handle(json!({
        "jsonrpc": "2.0",
        "id": 32,
        "method": "tools/call",
        "params": {
            "name": "kmp_ingest",
            "arguments": {
                "about": "question:830ce83f",
                "idempotency_key": "ingest:830ce83f:1"
            }
        }
    }))
    .await;

    assert_eq!(response["result"]["isError"], true);
    assert!(
        response["result"]["content"][0]["text"]
            .as_str()
            .expect("tool error should include text")
            .contains("missing required object argument `memory`")
    );
}

#[tokio::test]
async fn unknown_tool_returns_tool_error() {
    let response = handle(json!({
        "jsonrpc": "2.0",
        "id": 28,
        "method": "tools/call",
        "params": {
            "name": "kernel_unknown",
            "arguments": {}
        }
    }))
    .await;

    assert_eq!(response["result"]["isError"], true);
    assert!(
        response["result"]["content"][0]["text"]
            .as_str()
            .expect("tool error should include text")
            .contains("unknown KMP tool")
    );
}

#[tokio::test]
async fn tools_call_requires_object_params_and_name() {
    let missing_params = handle(json!({
        "jsonrpc": "2.0",
        "id": 29,
        "method": "tools/call"
    }))
    .await;
    assert_eq!(missing_params["error"]["code"], -32602);
    assert_eq!(
        missing_params["error"]["message"],
        "tools/call requires object params"
    );

    let missing_name = handle(json!({
        "jsonrpc": "2.0",
        "id": 30,
        "method": "tools/call",
        "params": {}
    }))
    .await;
    assert_eq!(missing_name["error"]["code"], -32602);
    assert_eq!(
        missing_name["error"]["message"],
        "tools/call requires params.name"
    );
}

#[tokio::test]
async fn tools_call_without_id_has_no_response() {
    let server = KernelMcpServer::fixture();
    let response = server
        .handle_json_line(
            &json!({
                "jsonrpc": "2.0",
                "method": "tools/call",
                "params": {
                    "name": "kmp_ask",
                    "arguments": {
                        "about": "question:830ce83f",
                        "question": "Where did Rachel move?"
                    }
                }
            })
            .to_string(),
        )
        .await;

    assert!(response.is_none());
}

#[tokio::test]
async fn grpc_backend_returns_tool_error_when_live_kernel_is_unavailable() {
    let server = KernelMcpServer::grpc("http://127.0.0.1:1");
    let response = handle_with(
        &server,
        json!({
            "jsonrpc": "2.0",
            "id": 31,
            "method": "tools/call",
            "params": {
                "name": "kmp_inspect",
                "arguments": {
                    "ref": "node:missing"
                }
            }
        }),
    )
    .await;

    assert_eq!(response["result"]["isError"], true);
    assert!(
        response["result"]["content"][0]["text"]
            .as_str()
            .expect("tool error should include text")
            .contains("failed to connect to kernel gRPC endpoint")
    );
}

#[tokio::test]
async fn invalid_tool_arguments_return_tool_error() {
    let response = handle(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "kmp_trace",
            "arguments": {
                "from": "claim:rachel-austin"
            }
        }
    }))
    .await;

    assert_eq!(response["result"]["isError"], true);
    assert!(
        response["result"]["content"][0]["text"]
            .as_str()
            .expect("tool error should include text")
            .contains("missing required argument `to`")
    );
}

#[tokio::test]
async fn initialized_notification_has_no_response() {
    let server = KernelMcpServer::fixture();
    let response = server
        .handle_json_line(
            &json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized",
                "params": {}
            })
            .to_string(),
        )
        .await;

    assert!(response.is_none());
}

#[tokio::test]
async fn server_can_use_injected_stub_backend() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let server = KernelMcpServer::with_backend(StubBackend {
        calls: Arc::clone(&calls),
        backend_name: "stub",
        grpc_tls_mode_name: "disabled",
        response: Ok(json!({
            "content": [
                {
                    "type": "text",
                    "text": "stub response"
                }
            ],
            "structuredContent": {
                "source": "stub"
            },
            "isError": false
        })),
    });

    let initialize = handle_with(
        &server,
        json!({
            "jsonrpc": "2.0",
            "id": 40,
            "method": "initialize",
            "params": {}
        }),
    )
    .await;
    assert_eq!(initialize["result"]["metadata"]["backend"], "stub");

    let response = handle_with(
        &server,
        json!({
            "jsonrpc": "2.0",
            "id": 41,
            "method": "tools/call",
            "params": {
                "name": "kmp_wake",
                "arguments": {
                    "about": "node:stub"
                }
            }
        }),
    )
    .await;

    assert_eq!(response["result"]["isError"], false);
    assert_eq!(response["result"]["structuredContent"]["source"], "stub");
    assert_eq!(
        calls
            .lock()
            .expect("stub calls should be available")
            .as_slice(),
        [("kmp_wake".to_string(), json!({"about": "node:stub"}))]
    );
}

#[tokio::test]
async fn server_wraps_injected_backend_errors_as_mcp_tool_errors() {
    let server = KernelMcpServer::with_backend(StubBackend {
        calls: Arc::new(Mutex::new(Vec::new())),
        backend_name: "stub",
        grpc_tls_mode_name: "mutual",
        response: Err(kmp_mcp::ToolError::backend("stub failure")),
    });

    assert_eq!(server.backend_name(), "stub");
    assert_eq!(server.grpc_tls_mode_name(), "mutual");

    let response = handle_with(
        &server,
        json!({
            "jsonrpc": "2.0",
            "id": 42,
            "method": "tools/call",
            "params": {
                "name": "kmp_trace",
                "arguments": {
                    "from": "a",
                    "to": "b"
                }
            }
        }),
    )
    .await;

    assert_eq!(response["result"]["isError"], true);
    assert_eq!(response["result"]["content"][0]["text"], "stub failure");
}

#[tokio::test]
async fn shared_stub_backend_can_be_reused_by_multiple_servers() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let backend = Arc::new(StubBackend {
        calls: Arc::clone(&calls),
        backend_name: "shared-stub",
        grpc_tls_mode_name: "disabled",
        response: Ok(json!({
            "content": [],
            "structuredContent": {
                "shared": true
            },
            "isError": false
        })),
    });
    let server_a = KernelMcpServer::with_shared_backend(backend.clone());
    let server_b = KernelMcpServer::with_shared_backend(backend);

    let response_a = call_named_tool(&server_a, 43, "kmp_inspect").await;
    let response_b = call_named_tool(&server_b, 44, "kmp_ask").await;

    assert_eq!(response_a["result"]["structuredContent"]["shared"], true);
    assert_eq!(response_b["result"]["structuredContent"]["shared"], true);
    assert_eq!(
        calls
            .lock()
            .expect("shared stub calls should be available")
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        vec!["kmp_inspect", "kmp_ask"]
    );
}

async fn handle(request: Value) -> Value {
    let server = KernelMcpServer::fixture();
    handle_with(&server, request).await
}

async fn handle_with(server: &KernelMcpServer, request: Value) -> Value {
    let response = server
        .handle_json_line(&request.to_string())
        .await
        .expect("request should produce a response");
    serde_json::from_str(&response).expect("response should be JSON")
}

async fn call_named_tool(server: &KernelMcpServer, id: u64, name: &str) -> Value {
    handle_with(
        server,
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": {}
            }
        }),
    )
    .await
}

fn sample_ingest_arguments() -> Value {
    serde_json::from_str(include_str!(
        "../../../api/examples/kernel/v1beta1/kmp/ingest.request.json"
    ))
    .expect("ingest fixture request should be valid JSON")
}

fn sample_write_arguments(dry_run: bool) -> Value {
    let mut request = serde_json::from_str::<Value>(include_str!(
        "../../../api/examples/kernel/v1beta1/kmp/write-memory.request.json"
    ))
    .expect("write fixture request should be valid JSON");
    request["options"]["dry_run"] = json!(dry_run);
    request
}

struct StubBackend {
    calls: Arc<Mutex<Vec<(String, Value)>>>,
    backend_name: &'static str,
    grpc_tls_mode_name: &'static str,
    response: Result<Value, kmp_mcp::ToolError>,
}

impl KernelMcpToolBackend for StubBackend {
    fn backend_name(&self) -> &'static str {
        self.backend_name
    }

    fn grpc_tls_mode_name(&self) -> &'static str {
        self.grpc_tls_mode_name
    }

    fn call_tool<'a>(&'a self, name: &'a str, arguments: &'a Value) -> KernelMcpToolFuture<'a> {
        self.calls
            .lock()
            .expect("stub calls should be available")
            .push((name.to_string(), arguments.clone()));
        let response = self.response.clone();
        Box::pin(async move { response })
    }
}
