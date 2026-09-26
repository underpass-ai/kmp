use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::json;

use super::typesafe_api_key::TypeSafeApiKey;
use super::typesafe_fixture::{Reply, serve};
use super::typesafe_judgement::TypeSafeJudgement;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::ports::judgement_model::JudgementModel;

const KEY: &str = "apikey_fixture_secret";

fn ok(body: serde_json::Value) -> Reply {
    Reply {
        status: 200,
        headers: Vec::new(),
        body: body.to_string(),
        delay: Duration::ZERO,
    }
}

fn status(code: u16, headers: Vec<(&'static str, String)>) -> Reply {
    Reply {
        status: code,
        headers,
        body: "{\"error\":\"echo apikey_fixture_secret\"}".into(),
        delay: Duration::ZERO,
    }
}

fn adapter(url: reqwest::Url, timeout: Duration) -> TypeSafeJudgement {
    TypeSafeJudgement::new(
        url,
        "jev-1.13.0".into(),
        TypeSafeApiKey::from_env(Some(KEY.into())).expect("key"),
        timeout,
    )
    .expect("adapter")
}

fn request() -> JudgementRequest {
    JudgementRequest {
        state: json!("The cat sat on the mat."),
        questions: BTreeMap::from([(
            "animal".into(),
            JudgementQuestion::Noul {
                instructions: json!("Does the text mention an animal?"),
            },
        )]),
    }
}

fn answer() -> serde_json::Value {
    json!({"model": "jev-1.13.0", "answers": {"animal": {"type": "noul", "noul": 0.99}},
           "usage": {"input_tokens": 280, "output_tokens": 20}})
}

#[tokio::test]
async fn a_judgement_sends_the_bearer_key_and_pinned_model_and_returns_typed_answers() {
    let (url, fixture) = serve(vec![ok(answer())]);
    let response = adapter(url, Duration::from_secs(5))
        .evaluate(&request())
        .await
        .expect("answered");
    let received = fixture.join().expect("fixture");
    assert_eq!(
        response.answers["animal"],
        JudgementAnswer::Noul { yes: 0.99 }
    );
    assert_eq!((response.input_tokens, response.requests), (280, 1));
    assert!(
        received[0]
            .0
            .to_ascii_lowercase()
            .contains(&format!("authorization: bearer {KEY}"))
    );
    assert_eq!(received[0].1["model"], "jev-1.13.0");
    assert_eq!(received[0].1["questions"]["animal"]["type"], "noul");
}

#[tokio::test]
async fn a_rejected_key_fails_without_echoing_key_or_body() {
    let (url, fixture) = serve(vec![status(401, Vec::new())]);
    let error = adapter(url, Duration::from_secs(5))
        .evaluate(&request())
        .await
        .expect_err("rejected");
    fixture.join().expect("fixture");
    assert!(error.contains("401"));
    assert!(!error.contains(KEY));
}

#[tokio::test]
async fn rate_limits_are_retried_after_the_announced_delay() {
    let (url, fixture) = serve(vec![
        status(429, vec![("Retry-After", "0".into())]),
        ok(answer()),
    ]);
    let response = adapter(url, Duration::from_secs(5))
        .evaluate(&request())
        .await
        .expect("retried");
    assert_eq!(fixture.join().expect("fixture").len(), 2);
    assert_eq!(response.requests, 1);
}

#[tokio::test]
async fn a_persistent_rate_limit_gives_up_after_two_retries() {
    let limited = || status(429, vec![("Retry-After", "0".into())]);
    let (url, fixture) = serve(vec![limited(), limited(), limited()]);
    let error = adapter(url, Duration::from_secs(5))
        .evaluate(&request())
        .await
        .expect_err("gave up");
    assert_eq!(fixture.join().expect("fixture").len(), 3);
    assert!(error.contains("rate limit"));
}

#[tokio::test]
async fn malformed_bodies_and_slow_answers_fail_safely() {
    let (url, fixture) = serve(vec![Reply {
        status: 200,
        headers: Vec::new(),
        body: "not json".into(),
        delay: Duration::ZERO,
    }]);
    assert!(
        adapter(url, Duration::from_secs(5))
            .evaluate(&request())
            .await
            .is_err()
    );
    fixture.join().expect("fixture");

    let (url, fixture) = serve(vec![Reply {
        delay: Duration::from_millis(1_500),
        ..ok(answer())
    }]);
    let error = adapter(url, Duration::from_millis(300))
        .evaluate(&request())
        .await
        .expect_err("timed out");
    assert!(error.contains("timed out"));
    fixture.join().expect("fixture");
}

#[tokio::test]
async fn an_empty_request_makes_no_call() {
    let (url, fixture) = serve(Vec::new());
    let response = adapter(url, Duration::from_secs(5))
        .evaluate(&JudgementRequest {
            state: json!("s"),
            questions: BTreeMap::new(),
        })
        .await
        .expect("empty");
    assert_eq!(response.requests, 0);
    assert!(fixture.join().expect("fixture").is_empty());
}

#[test]
fn loading_is_off_without_a_file_and_refused_without_a_key() {
    let dir = tempfile::tempdir().expect("dir");
    assert!(
        TypeSafeJudgement::load(dir.path(), Some(KEY.into()))
            .expect("no file")
            .is_none()
    );
    std::fs::write(
        dir.path().join("typesafe.json"),
        r#"{"endpoint":"https://api.typesafe.ai/v1/systemone","model":"jev-1.13.0","timeout_ms":20000}"#,
    )
    .expect("config");
    let absent = dir.path().join("absent-typesafe.env");
    let error = TypeSafeJudgement::load_with(dir.path(), None, |key| {
        TypeSafeApiKey::load_from(key, Some(absent))
    })
    .err()
    .expect("no key");
    assert!(error.contains("TYPESAFE_API_KEY"));
    let loaded = TypeSafeJudgement::load(dir.path(), Some(KEY.into()))
        .expect("loads")
        .expect("configured");
    assert_eq!(loaded.model(), "jev-1.13.0");
}

/// Operator path: one real call. Run with
/// `TYPESAFE_API_KEY=… cargo test -p kmp-mcp --lib typesafe_live -- --ignored`.
#[tokio::test]
#[ignore = "calls the real TypeSafe API with TYPESAFE_API_KEY"]
async fn typesafe_live_answers_a_harmless_question() {
    let adapter = TypeSafeJudgement::new(
        reqwest::Url::parse("https://api.typesafe.ai/v1/systemone").expect("url"),
        "jev-1.13.0".into(),
        TypeSafeApiKey::from_env(std::env::var("TYPESAFE_API_KEY").ok()).expect("key"),
        Duration::from_secs(20),
    )
    .expect("adapter");
    let response = adapter.evaluate(&request()).await.expect("live answer");
    match &response.answers["animal"] {
        JudgementAnswer::Noul { yes } => assert!(*yes > 0.5),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn debug_output_never_contains_the_key() {
    let (url, _fixture) = serve(Vec::new());
    let rendered = format!("{:?}", adapter(url, Duration::from_secs(5)));
    assert!(!rendered.contains(KEY));
}

/// A provider that answers every question of each request with 0.5 after
/// `delay`, serving connections concurrently; returns the most requests it
/// held at once and how many it served.
async fn concurrent_provider(
    delay: Duration,
) -> (
    reqwest::Url,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
) {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let url = reqwest::Url::parse(&format!(
        "http://{}/v1/systemone",
        listener.local_addr().expect("address")
    ))
    .expect("url");
    let (now, peak, served) = (
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicUsize::new(0)),
    );
    let (peak_out, served_out) = (Arc::clone(&peak), Arc::clone(&served));
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let (now, peak, served) = (Arc::clone(&now), Arc::clone(&peak), Arc::clone(&served));
            tokio::spawn(async move {
                let mut data = Vec::new();
                let mut byte = [0u8; 1];
                while !data.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).await.expect("headers");
                    data.push(byte[0]);
                }
                let length = String::from_utf8_lossy(&data)
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|n| n.trim().parse::<usize>().expect("length"))
                    })
                    .expect("length");
                let mut body = vec![0u8; length];
                stream.read_exact(&mut body).await.expect("body");
                let at_once = now.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(at_once, Ordering::SeqCst);
                tokio::time::sleep(delay).await;
                now.fetch_sub(1, Ordering::SeqCst);
                served.fetch_add(1, Ordering::SeqCst);
                let request: serde_json::Value = serde_json::from_slice(&body).expect("json");
                let answers = request["questions"]
                    .as_object()
                    .expect("questions")
                    .keys()
                    .map(|key| (key.clone(), json!({"type": "noul", "noul": 0.5})))
                    .collect::<serde_json::Map<_, _>>();
                let reply = json!({"model": "jev-1.13.0", "answers": answers,
                                   "usage": {"input_tokens": 10}})
                .to_string();
                let _ = stream
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                            reply.len()
                        )
                        .as_bytes(),
                    )
                    .await;
            });
        }
    });
    (url, peak_out, served_out)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn batches_go_out_in_parallel_within_the_concurrency_limit() {
    use std::sync::atomic::Ordering;
    let (url, peak, served) = concurrent_provider(Duration::from_millis(150)).await;
    // Twenty ~40 KB questions: five batches under the 60k-token budget.
    let questions = (0..20)
        .map(|n| {
            (
                format!("q{n:02}"),
                JudgementQuestion::Noul {
                    instructions: json!(format!("{n} {}", "word ".repeat(8_000))),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let started = std::time::Instant::now();
    let response = adapter(url, Duration::from_secs(10))
        .evaluate(&JudgementRequest {
            state: json!("s"),
            questions,
        })
        .await
        .expect("answered");
    assert_eq!(response.answers.len(), 20);
    assert_eq!(response.requests, 5);
    assert_eq!(response.input_tokens, 50);
    assert_eq!(served.load(Ordering::SeqCst), 5);
    let peak = peak.load(Ordering::SeqCst);
    assert!(
        (2..=super::typesafe_judgement::CONCURRENT_REQUESTS).contains(&peak),
        "{peak} requests at once"
    );
    assert!(
        started.elapsed() < Duration::from_millis(700),
        "five sequential batches take 750 ms: {:?}",
        started.elapsed()
    );
}
