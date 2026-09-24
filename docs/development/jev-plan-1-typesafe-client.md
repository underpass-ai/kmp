# TypeSafe judgement client — implementation plan (1 of 3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a port `JudgementModel` and a TypeSafe Jev adapter to
`kmp-mcp`. The adapter is opt-in per store, takes its key from
`TYPESAFE_API_KEY`, is bounded, fails safe, and is tested against a local
HTTP fixture. `kmp_curate` (plan 2) and Ask re-ranking (plan 3) will consume
it.

**Architecture:** The port lives in `serving/ports`, the value types in
`serving`, and the adapter files in `serving/adapters`, one primary type per
file. The adapter mirrors `loopback_semantic_retriever.rs`: config beside the
store, `Ok(None)` when absent, `reqwest` with no proxy and no redirects, and
capped bodies. The adapter splits questions into requests that fit the
provider's token budget and checks every answer against the question it
answers.

**Tech Stack:** Rust 2024, `reqwest` 0.12 (workspace, rustls), `tokio`,
`serde`/`serde_json`. There are no new dependencies.

**Spec:** `docs/development/jev-curate-design.md`, section "Shared TypeSafe
client (PR 1)". Provider contract:
`https://docs.typesafe.ai/api.md`.

## Global Constraints

- There are no new crates or dependencies. Use the workspace `reqwest`,
  `tokio`, `serde`, `serde_json` and `sha2` already in `crates/kmp-mcp/Cargo.toml`.
- Endpoint: `https://api.typesafe.ai/v1/systemone`, host `api.typesafe.ai`,
  HTTPS only, with no port, credentials, query or fragment.
- Model: pinned, 1–64 characters from `[A-Za-z0-9._-]`, and never ending in
  `-latest`.
- `timeout_ms`: 1000–60000. Connect timeout 3 s.
- The config file is `typesafe.json` beside the store, at most 8192 bytes,
  with `deny_unknown_fields`.
- The key comes from `TYPESAFE_API_KEY` only. It never appears in a log,
  warning, error, `Debug` output or test assertion message.
- Requests and responses are capped at 256 KiB. On `429` there are at most
  two retries, honouring `retry-after` (in seconds) capped at 5 s.
- Budget: estimated state + longest question ≤ 32,000 tokens, and each
  request ≤ 60,000 tokens. The estimate is JSON bytes ÷ 3, rounded up.
- A `choice` has 2–255 options.
- Code: one primary type per file, and each file ≤ 600 lines
  (`scripts/ci/kmp-mcp-architecture-gate.sh`). CI runs clippy with
  `-D warnings`.
- **Dead code until plan 2:** nothing consumes the port yet. Each new
  `mod` line carries
  `#[allow(dead_code)] // consumed by kmp_curate (jev-curate plan 2)`, and
  plan 2's first task removes those attributes. Do not push this branch or
  open a PR before plan 2 has a consumer.
- Commits: `feat(mcp): …` / `test(mcp): …`, one per task, on branch
  `feat/jev-curate` in worktree `~/Documents/ai/kmp-jev-curate`.

## File structure

| File | Responsibility |
| --- | --- |
| `crates/kmp-mcp/src/serving/judgement_question.rs` | `JudgementQuestion`: one noul or choice question |
| `crates/kmp-mcp/src/serving/judgement_request.rs` | `JudgementRequest`: one state and named questions |
| `crates/kmp-mcp/src/serving/judgement_answer.rs` | `JudgementAnswer`: a validated noul or choice answer |
| `crates/kmp-mcp/src/serving/judgement_response.rs` | `JudgementResponse`: answers merged across requests, plus model and usage |
| `crates/kmp-mcp/src/serving/ports/judgement_model.rs` | `JudgementModel` port |
| `crates/kmp-mcp/src/serving/adapters/typesafe_request_body.rs` | Builds the provider JSON body |
| `crates/kmp-mcp/src/serving/adapters/typesafe_wire_response.rs` | `TypeSafeWireResponse`: parses and validates the provider body against the questions sent |
| `crates/kmp-mcp/src/serving/adapters/typesafe_config.rs` | `TypeSafeConfig`: `typesafe.json` and its validation |
| `crates/kmp-mcp/src/serving/adapters/typesafe_api_key.rs` | `TypeSafeApiKey`: a redacted bearer credential |
| `crates/kmp-mcp/src/serving/adapters/typesafe_batches.rs` | Splits questions into requests within budget |
| `crates/kmp-mcp/src/serving/adapters/typesafe_judgement.rs` | `TypeSafeJudgement`: HTTP, retries, loader, and the port implementation |
| `crates/kmp-mcp/src/serving/adapters/typesafe_fixture.rs` | Test-only scripted HTTP server |
| `crates/kmp-mcp/src/serving/adapters/typesafe_judgement_tests.rs` | Adapter tests against the fixture |

---

### Task 1: Judgement port, value types, request body and response validation

**Files:**
- Create: `crates/kmp-mcp/src/serving/judgement_question.rs`, `judgement_request.rs`, `judgement_answer.rs`, `judgement_response.rs`
- Create: `crates/kmp-mcp/src/serving/ports/judgement_model.rs`
- Create: `crates/kmp-mcp/src/serving/adapters/typesafe_request_body.rs`, `typesafe_wire_response.rs`
- Modify: `crates/kmp-mcp/src/serving/mod.rs` (add four `pub(crate) mod` lines next to `semantic_retrieval_outcome`)
- Modify: `crates/kmp-mcp/src/serving/ports/mod.rs` (add `pub(crate) mod judgement_model;`)
- Modify: `crates/kmp-mcp/src/serving/adapters/mod.rs` (add `mod typesafe_request_body;` and `mod typesafe_wire_response;`)

**Interfaces:**
- Produces:
  - `JudgementQuestion::{Noul { instructions: Value }, Choice { instructions: Value, options: Vec<String> }}`
  - `JudgementRequest { state: Value, questions: BTreeMap<String, JudgementQuestion> }`
  - `JudgementAnswer::{Noul { yes: f64 }, Choice { choice: String, probabilities: BTreeMap<String, f64>, confidence: f64 }}`
  - `JudgementResponse { model: String, answers: BTreeMap<String, JudgementAnswer>, input_tokens: u64, requests: usize }`
  - The trait `JudgementModel` with `fn model(&self) -> &str` and
    `fn evaluate<'a>(&'a self, request: &'a JudgementRequest) -> Pin<Box<dyn Future<Output = Result<JudgementResponse, String>> + Send + 'a>>`
  - `typesafe_request_body(model: &str, state: &Value, questions: &BTreeMap<String, JudgementQuestion>) -> Value`
  - `TypeSafeWireResponse::into_response(self, model: &str, questions: &BTreeMap<String, JudgementQuestion>) -> Result<JudgementResponse, String>`

- [ ] **Step 1: Write the value types and the port**

`serving/judgement_question.rs`:

```rust
use serde_json::Value;

/// One typed question for a judgement model. `instructions` may be a string
/// or structured JSON that names fields in backticks.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum JudgementQuestion {
    /// Yes/no: the answer is the probability of yes.
    Noul { instructions: Value },
    /// One option of a closed set: the answer is the choice plus the
    /// distribution over every option offered.
    Choice {
        instructions: Value,
        options: Vec<String>,
    },
}
```

`serving/judgement_request.rs`:

```rust
use std::collections::BTreeMap;

use serde_json::Value;

use super::judgement_question::JudgementQuestion;

/// One state judged by named questions. Keys are chosen by the caller and
/// come back unchanged; they are not shown to the model.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JudgementRequest {
    pub state: Value,
    pub questions: BTreeMap<String, JudgementQuestion>,
}
```

`serving/judgement_answer.rs`:

```rust
use std::collections::BTreeMap;

/// A validated answer: probabilities are finite and within [0, 1], and a
/// choice is one of the options offered.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum JudgementAnswer {
    Noul {
        yes: f64,
    },
    Choice {
        choice: String,
        probabilities: BTreeMap<String, f64>,
        confidence: f64,
    },
}
```

`serving/judgement_response.rs`:

```rust
use std::collections::BTreeMap;

use super::judgement_answer::JudgementAnswer;

/// Answers for every question of one request, merged across the provider
/// calls the budget required.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JudgementResponse {
    pub model: String,
    pub answers: BTreeMap<String, JudgementAnswer>,
    pub input_tokens: u64,
    pub requests: usize,
}
```

`serving/ports/judgement_model.rs`:

```rust
use std::{future::Future, pin::Pin};

use crate::serving::judgement_request::JudgementRequest;
use crate::serving::judgement_response::JudgementResponse;

/// Optional remote judgement. It answers typed questions about text it is
/// given; it never writes memory, and its answers are proposals the caller
/// validates and a writer justifies.
pub(crate) trait JudgementModel: Send + Sync {
    /// The pinned model every answer must come from.
    fn model(&self) -> &str;

    fn evaluate<'a>(
        &'a self,
        request: &'a JudgementRequest,
    ) -> Pin<Box<dyn Future<Output = Result<JudgementResponse, String>> + Send + 'a>>;
}
```

In `serving/mod.rs`, next to `pub(crate) mod semantic_retrieval_outcome;`, add:

```rust
#[allow(dead_code)] // consumed by kmp_curate (jev-curate plan 2)
pub(crate) mod judgement_answer;
#[allow(dead_code)] // consumed by kmp_curate (jev-curate plan 2)
pub(crate) mod judgement_question;
#[allow(dead_code)] // consumed by kmp_curate (jev-curate plan 2)
pub(crate) mod judgement_request;
#[allow(dead_code)] // consumed by kmp_curate (jev-curate plan 2)
pub(crate) mod judgement_response;
```

In `serving/ports/mod.rs`:

```rust
#[allow(dead_code)] // consumed by kmp_curate (jev-curate plan 2)
pub(crate) mod judgement_model;
```

- [ ] **Step 2: Write the failing tests for the body and the response validation**

`serving/adapters/typesafe_request_body.rs`, with the test first and the
function body left as `todo!()`:

```rust
use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::serving::judgement_question::JudgementQuestion;

/// The provider body for one batch: `state`, the pinned `model`, and one
/// typed question per key. Choice options carry no description.
pub(super) fn typesafe_request_body(
    model: &str,
    state: &Value,
    questions: &BTreeMap<String, JudgementQuestion>,
) -> Value {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_names_the_pinned_model_and_keeps_question_keys() {
        let questions = BTreeMap::from([
            (
                "c0".to_string(),
                JudgementQuestion::Noul {
                    instructions: json!("Does it rain?"),
                },
            ),
            (
                "c1".to_string(),
                JudgementQuestion::Choice {
                    instructions: json!({"question": "Which?"}),
                    options: vec!["a".into(), "none".into()],
                },
            ),
        ]);
        let body = typesafe_request_body("jev-1.13.0", &json!("state"), &questions);
        assert_eq!(
            body,
            json!({
                "state": "state",
                "model": "jev-1.13.0",
                "questions": {
                    "c0": {"type": "noul", "instructions": "Does it rain?"},
                    "c1": {"type": "choice", "instructions": {"question": "Which?"},
                           "criteria": {"a": null, "none": null}}
                }
            })
        );
    }
}
```

`serving/adapters/typesafe_wire_response.rs`, with the tests first and the
method body left as `todo!()`:

```rust
use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_response::JudgementResponse;

/// The provider body as received. Nothing in it is trusted until
/// `into_response` has matched every answer to the question it answers.
#[derive(Debug, Deserialize)]
pub(super) struct TypeSafeWireResponse {
    model: String,
    answers: BTreeMap<String, Value>,
    #[serde(default)]
    usage: Value,
}

impl TypeSafeWireResponse {
    pub(super) fn into_response(
        self,
        model: &str,
        questions: &BTreeMap<String, JudgementQuestion>,
    ) -> Result<JudgementResponse, String> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn questions() -> BTreeMap<String, JudgementQuestion> {
        BTreeMap::from([
            (
                "n".to_string(),
                JudgementQuestion::Noul {
                    instructions: json!("q"),
                },
            ),
            (
                "c".to_string(),
                JudgementQuestion::Choice {
                    instructions: json!("q"),
                    options: vec!["a".into(), "b".into()],
                },
            ),
        ])
    }

    fn wire(body: Value) -> TypeSafeWireResponse {
        serde_json::from_value(body).expect("wire body")
    }

    fn valid() -> Value {
        json!({
            "model": "jev-1.13.0",
            "answers": {
                "n": {"type": "noul", "noul": 0.95},
                "c": {"type": "choice", "choice": "a",
                      "probabilities": {"a": 0.8, "b": 0.2}, "confidence": 0.7}
            },
            "usage": {"input_tokens": 296, "output_tokens": 20}
        })
    }

    #[test]
    fn valid_answers_are_typed_and_usage_is_kept() {
        let response = wire(valid())
            .into_response("jev-1.13.0", &questions())
            .expect("valid");
        assert_eq!(response.input_tokens, 296);
        assert_eq!(response.requests, 1);
        assert_eq!(response.answers["n"], JudgementAnswer::Noul { yes: 0.95 });
        assert_eq!(
            response.answers["c"],
            JudgementAnswer::Choice {
                choice: "a".into(),
                probabilities: BTreeMap::from([("a".into(), 0.8), ("b".into(), 0.2)]),
                confidence: 0.7
            }
        );
    }

    #[test]
    fn answers_that_do_not_match_the_questions_are_rejected() {
        let mut cases = Vec::new();
        let mut other_model = valid();
        other_model["model"] = json!("jev-1.14.0");
        cases.push(other_model);
        let mut missing = valid();
        missing["answers"].as_object_mut().expect("answers").remove("n");
        cases.push(missing);
        let mut extra = valid();
        extra["answers"]["x"] = json!({"type": "noul", "noul": 0.1});
        cases.push(extra);
        let mut out_of_range = valid();
        out_of_range["answers"]["n"]["noul"] = json!(1.2);
        cases.push(out_of_range);
        let mut wrong_type = valid();
        wrong_type["answers"]["n"] = valid()["answers"]["c"].clone();
        cases.push(wrong_type);
        let mut not_offered = valid();
        not_offered["answers"]["c"]["choice"] = json!("z");
        cases.push(not_offered);
        let mut partial = valid();
        partial["answers"]["c"]["probabilities"] = json!({"a": 1.0});
        cases.push(partial);
        let mut no_confidence = valid();
        no_confidence["answers"]["c"]
            .as_object_mut()
            .expect("choice")
            .remove("confidence");
        cases.push(no_confidence);
        for case in cases {
            assert!(
                wire(case.clone())
                    .into_response("jev-1.13.0", &questions())
                    .is_err(),
                "accepted {case}"
            );
        }
    }
}
```

In `serving/adapters/mod.rs`, add after `mod semantic_retriever_config;`:

```rust
#[allow(dead_code)] // consumed by kmp_curate (jev-curate plan 2)
mod typesafe_request_body;
#[allow(dead_code)] // consumed by kmp_curate (jev-curate plan 2)
mod typesafe_wire_response;
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p kmp-mcp --lib typesafe_ -- --nocapture`
Expected: FAIL, panicking with `not yet implemented`.

- [ ] **Step 4: Implement the body builder**

Replace the `todo!()` in `typesafe_request_body`:

```rust
    let questions = questions
        .iter()
        .map(|(key, question)| {
            let typed = match question {
                JudgementQuestion::Noul { instructions } => {
                    json!({"type": "noul", "instructions": instructions})
                }
                JudgementQuestion::Choice {
                    instructions,
                    options,
                } => json!({
                    "type": "choice",
                    "instructions": instructions,
                    "criteria": options
                        .iter()
                        .map(|option| (option.clone(), Value::Null))
                        .collect::<Map<_, _>>(),
                }),
            };
            (key.clone(), typed)
        })
        .collect::<Map<_, _>>();
    json!({"state": state, "model": model, "questions": questions})
```

- [ ] **Step 5: Implement the response validation**

Replace the `todo!()` in `into_response`, and add the two private helpers
below the `impl` block:

```rust
        if self.model != model {
            return Err("TypeSafe answered with a different model than configured".into());
        }
        if self.answers.len() != questions.len()
            || !questions.keys().all(|key| self.answers.contains_key(key))
        {
            return Err("TypeSafe answers do not match the questions sent".into());
        }
        let mut answers = BTreeMap::new();
        for (key, question) in questions {
            answers.insert(key.clone(), typed_answer(&self.answers[key], question)?);
        }
        Ok(JudgementResponse {
            model: self.model,
            answers,
            input_tokens: self
                .usage
                .get("input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            requests: 1,
        })
```

```rust
fn probability(value: Option<&Value>) -> Result<f64, String> {
    value
        .and_then(Value::as_f64)
        .filter(|p| p.is_finite() && (0.0..=1.0).contains(p))
        .ok_or_else(|| "TypeSafe returned a probability outside [0, 1]".to_string())
}

fn typed_answer(raw: &Value, question: &JudgementQuestion) -> Result<JudgementAnswer, String> {
    match question {
        JudgementQuestion::Noul { .. } => {
            if raw.get("type").and_then(Value::as_str) != Some("noul") {
                return Err("TypeSafe answered a noul with another type".into());
            }
            Ok(JudgementAnswer::Noul {
                yes: probability(raw.get("noul"))?,
            })
        }
        JudgementQuestion::Choice { options, .. } => {
            if raw.get("type").and_then(Value::as_str) != Some("choice") {
                return Err("TypeSafe answered a choice with another type".into());
            }
            let choice = raw
                .get("choice")
                .and_then(Value::as_str)
                .filter(|choice| options.iter().any(|option| option == choice))
                .ok_or("TypeSafe chose an option that was not offered")?
                .to_string();
            let listed = raw
                .get("probabilities")
                .and_then(Value::as_object)
                .filter(|listed| listed.len() == options.len())
                .ok_or("TypeSafe probabilities do not cover the options offered")?;
            let mut probabilities = BTreeMap::new();
            for option in options {
                probabilities.insert(option.clone(), probability(listed.get(option))?);
            }
            Ok(JudgementAnswer::Choice {
                choice,
                probabilities,
                confidence: probability(raw.get("confidence"))?,
            })
        }
    }
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p kmp-mcp --lib typesafe_`
Expected: PASS (3 tests).

- [ ] **Step 7: Lint and commit**

```bash
cargo fmt -p kmp-mcp && cargo clippy -p kmp-mcp --all-targets -- -D warnings
git add crates/kmp-mcp/src/serving
git commit -m "feat(mcp): add judgement port and TypeSafe wire validation"
```

---

### Task 2: Config and redacted API key

**Files:**
- Create: `crates/kmp-mcp/src/serving/adapters/typesafe_config.rs`
- Create: `crates/kmp-mcp/src/serving/adapters/typesafe_api_key.rs`
- Modify: `crates/kmp-mcp/src/serving/adapters/mod.rs`
- Modify: `crates/kmp-mcp/src/serving/environment.rs` (add the variable name)

**Interfaces:**
- Produces:
  - `TypeSafeConfig { endpoint: String, model: String, timeout_ms: u64 }`
    with `validate(&self) -> Result<(reqwest::Url, Duration), String>`
  - `TypeSafeApiKey::from_env(Option<String>) -> Result<TypeSafeApiKey, String>`
    and `header(&self) -> reqwest::header::HeaderValue`
  - `pub const TYPESAFE_API_KEY_ENV: &str = "TYPESAFE_API_KEY";` in
    `serving/environment.rs`

- [ ] **Step 1: Write the failing tests**

`serving/adapters/typesafe_config.rs`:

```rust
use std::time::Duration;

use serde::Deserialize;

/// Explicit per-store opt-in for TypeSafe judgement. Holds no credential.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TypeSafeConfig {
    pub endpoint: String,
    pub model: String,
    pub timeout_ms: u64,
}

impl TypeSafeConfig {
    pub(super) fn validate(&self) -> Result<(reqwest::Url, Duration), String> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(endpoint: &str, model: &str, timeout_ms: u64) -> TypeSafeConfig {
        TypeSafeConfig {
            endpoint: endpoint.into(),
            model: model.into(),
            timeout_ms,
        }
    }

    const ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";

    #[test]
    fn only_the_provider_host_a_pinned_model_and_a_bounded_timeout_validate() {
        let (url, timeout) = config(ENDPOINT, "jev-1.13.0", 20_000)
            .validate()
            .expect("valid");
        assert_eq!(url.as_str(), ENDPOINT);
        assert_eq!(timeout, Duration::from_millis(20_000));
        for bad in [
            config("http://api.typesafe.ai/v1/systemone", "jev-1.13.0", 20_000),
            config("https://example.com/v1/systemone", "jev-1.13.0", 20_000),
            config("https://api.typesafe.ai:8443/v1/systemone", "jev-1.13.0", 20_000),
            config("https://u:p@api.typesafe.ai/v1/systemone", "jev-1.13.0", 20_000),
            config("https://api.typesafe.ai/v1/systemone?x=1", "jev-1.13.0", 20_000),
            config(ENDPOINT, "jev-latest", 20_000),
            config(ENDPOINT, "", 20_000),
            config(ENDPOINT, "jev 1.13", 20_000),
            config(ENDPOINT, "jev-1.13.0", 999),
            config(ENDPOINT, "jev-1.13.0", 60_001),
        ] {
            assert!(bad.validate().is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let parsed = serde_json::from_str::<TypeSafeConfig>(
            r#"{"endpoint":"x","model":"y","timeout_ms":1000,"api_key":"z"}"#,
        );
        assert!(parsed.is_err());
    }
}
```

`serving/adapters/typesafe_api_key.rs`:

```rust
use reqwest::header::HeaderValue;

/// The TypeSafe bearer credential. Read from the environment only; never
/// serialized, and redacted from `Debug`.
pub(super) struct TypeSafeApiKey(HeaderValue);

impl TypeSafeApiKey {
    pub(super) fn from_env(value: Option<String>) -> Result<Self, String> {
        todo!()
    }

    pub(super) fn header(&self) -> HeaderValue {
        self.0.clone()
    }
}

impl std::fmt::Debug for TypeSafeApiKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("TypeSafeApiKey(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_or_malformed_key_is_refused_without_echoing_it() {
        assert!(TypeSafeApiKey::from_env(None).is_err());
        assert!(TypeSafeApiKey::from_env(Some(String::new())).is_err());
        for bad in ["has space", "line\nbreak"] {
            let error = TypeSafeApiKey::from_env(Some(bad.into())).expect_err("refused");
            assert!(!error.contains(bad), "echoed the key");
        }
    }

    #[test]
    fn the_key_is_a_sensitive_bearer_header_and_debug_redacts_it() {
        let key = TypeSafeApiKey::from_env(Some("apikey_secret".into())).expect("key");
        let header = key.header();
        assert!(header.is_sensitive());
        assert_eq!(header.to_str().expect("ascii"), "Bearer apikey_secret");
        assert!(!format!("{key:?}").contains("apikey_secret"));
    }
}
```

In `serving/adapters/mod.rs` add:

```rust
#[allow(dead_code)] // consumed by kmp_curate (jev-curate plan 2)
mod typesafe_api_key;
#[allow(dead_code)] // consumed by kmp_curate (jev-curate plan 2)
mod typesafe_config;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p kmp-mcp --lib typesafe_`
Expected: FAIL, panicking with `not yet implemented` in the two new modules.

- [ ] **Step 3: Implement `validate`**

```rust
        let url = reqwest::Url::parse(&self.endpoint).map_err(|_| "invalid TypeSafe endpoint")?;
        if url.scheme() != "https"
            || url.host_str() != Some("api.typesafe.ai")
            || url.port().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(
                "TypeSafe endpoint must be https://api.typesafe.ai without credentials, port, query or fragment"
                    .into(),
            );
        }
        let model = self.model.as_str();
        if model.is_empty()
            || model.len() > 64
            || model.ends_with("-latest")
            || !model
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
        {
            return Err("TypeSafe model must be a pinned version such as jev-1.13.0".into());
        }
        if !(1_000..=60_000).contains(&self.timeout_ms) {
            return Err("TypeSafe timeout_ms must be between 1000 and 60000".into());
        }
        Ok((url, Duration::from_millis(self.timeout_ms)))
```

- [ ] **Step 4: Implement `from_env`**

```rust
        let key = value
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "TYPESAFE_API_KEY is not set".to_string())?;
        if key.len() > 512 || key.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err("TYPESAFE_API_KEY is malformed".into());
        }
        let mut header = HeaderValue::from_str(&format!("Bearer {key}"))
            .map_err(|_| "TYPESAFE_API_KEY is malformed")?;
        header.set_sensitive(true);
        Ok(Self(header))
```

`"has space"` is trimmed to itself and refused for its inner whitespace.
`"line\nbreak"` is refused as a control character. `""` is refused as not
set.

- [ ] **Step 5: Add the variable name**

In `serving/environment.rs`, after `GRPC_TLS_DOMAIN_NAME_ENV`:

```rust
/// Bearer key for the optional TypeSafe judgement adapter. Never logged.
pub const TYPESAFE_API_KEY_ENV: &str = "TYPESAFE_API_KEY";
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p kmp-mcp --lib typesafe_`
Expected: PASS (7 tests).

- [ ] **Step 7: Lint and commit**

```bash
cargo fmt -p kmp-mcp && cargo clippy -p kmp-mcp --all-targets -- -D warnings
git add crates/kmp-mcp/src/serving
git commit -m "feat(mcp): add TypeSafe store config and redacted API key"
```

---

### Task 3: Budget-bounded batching

**Files:**
- Create: `crates/kmp-mcp/src/serving/adapters/typesafe_batches.rs`
- Modify: `crates/kmp-mcp/src/serving/adapters/mod.rs`

**Interfaces:**
- Consumes: `JudgementRequest` and `JudgementQuestion` (Task 1), and
  `typesafe_request_body` (Task 1)
- Produces:
  - `typesafe_batches(request: &JudgementRequest) -> Result<Vec<BTreeMap<String, JudgementQuestion>>, String>`
  - `pub(super) const REQUEST_BYTES: usize = 256 * 1024;`

- [ ] **Step 1: Write the failing tests**

```rust
use std::collections::BTreeMap;

use serde_json::Value;

use super::typesafe_request_body::typesafe_request_body;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;

pub(super) const STATE_AND_QUESTION_TOKENS: usize = 32_000;
pub(super) const REQUEST_TOKENS: usize = 60_000;
pub(super) const REQUEST_BYTES: usize = 256 * 1024;

/// Conservative: three bytes of JSON per token.
fn estimated_tokens(value: &Value) -> usize {
    value.to_string().len().div_ceil(3)
}

/// Splits the questions into requests that each repeat the state and fit the
/// provider's budgets, in key order.
pub(super) fn typesafe_batches(
    request: &JudgementRequest,
) -> Result<Vec<BTreeMap<String, JudgementQuestion>>, String> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn noul(text: &str) -> JudgementQuestion {
        JudgementQuestion::Noul {
            instructions: json!(text),
        }
    }

    #[test]
    fn small_requests_stay_whole_and_large_ones_split_in_key_order() {
        let small = JudgementRequest {
            state: json!("s"),
            questions: BTreeMap::from([("a".into(), noul("q")), ("b".into(), noul("q"))]),
        };
        assert_eq!(typesafe_batches(&small).expect("fits").len(), 1);

        let passage = "x".repeat(20_000);
        let large = JudgementRequest {
            state: json!("s"),
            questions: (0..20)
                .map(|n| (format!("c{n:02}"), noul(&passage)))
                .collect(),
        };
        let batches = typesafe_batches(&large).expect("splits");
        assert!(batches.len() > 1);
        let keys = batches
            .iter()
            .flat_map(|batch| batch.keys().cloned())
            .collect::<Vec<_>>();
        assert_eq!(keys, large.questions.keys().cloned().collect::<Vec<_>>());
    }

    #[test]
    fn over_budget_questions_and_bad_choices_are_refused() {
        let huge = JudgementRequest {
            state: json!("x".repeat(100_000)),
            questions: BTreeMap::from([("a".into(), noul("q"))]),
        };
        assert!(typesafe_batches(&huge).is_err());
        for options in [vec!["only".to_string()], (0..256).map(|n| n.to_string()).collect()] {
            let bad = JudgementRequest {
                state: json!("s"),
                questions: BTreeMap::from([(
                    "a".into(),
                    JudgementQuestion::Choice {
                        instructions: json!("q"),
                        options,
                    },
                )]),
            };
            assert!(typesafe_batches(&bad).is_err());
        }
    }

    #[test]
    fn no_questions_means_no_requests() {
        let empty = JudgementRequest {
            state: json!("s"),
            questions: BTreeMap::new(),
        };
        assert!(typesafe_batches(&empty).expect("empty").is_empty());
    }
}
```

In `serving/adapters/mod.rs` add:

```rust
#[allow(dead_code)] // consumed by kmp_curate (jev-curate plan 2)
mod typesafe_batches;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p kmp-mcp --lib typesafe_batches`
Expected: FAIL with `not yet implemented`.

- [ ] **Step 3: Implement**

```rust
    let state_tokens = estimated_tokens(&request.state);
    let mut batches = Vec::new();
    let mut current = BTreeMap::new();
    let mut current_tokens = state_tokens;
    for (key, question) in &request.questions {
        if let JudgementQuestion::Choice { options, .. } = question
            && !(2..=255).contains(&options.len())
        {
            return Err(format!("choice `{key}` needs between 2 and 255 options"));
        }
        let single = BTreeMap::from([(key.clone(), question.clone())]);
        let tokens = estimated_tokens(&typesafe_request_body("", &Value::Null, &single));
        if state_tokens + tokens > STATE_AND_QUESTION_TOKENS {
            return Err(format!(
                "state plus question `{key}` exceeds the 32k-token judgement budget"
            ));
        }
        if !current.is_empty() && current_tokens + tokens > REQUEST_TOKENS {
            batches.push(std::mem::take(&mut current));
            current_tokens = state_tokens;
        }
        current.insert(key.clone(), question.clone());
        current_tokens += tokens;
    }
    if !current.is_empty() {
        batches.push(current);
    }
    Ok(batches)
```

The `if let … && …` let-chain needs edition 2024. The workspace is
edition 2024 with Rust 1.97.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p kmp-mcp --lib typesafe_batches`
Expected: PASS (3 tests).

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt -p kmp-mcp && cargo clippy -p kmp-mcp --all-targets -- -D warnings
git add crates/kmp-mcp/src/serving/adapters
git commit -m "feat(mcp): split TypeSafe judgements within the provider budget"
```

---

### Task 4: HTTP adapter, retries and loader, tested against a local fixture

**Files:**
- Create: `crates/kmp-mcp/src/serving/adapters/typesafe_judgement.rs`
- Create: `crates/kmp-mcp/src/serving/adapters/typesafe_fixture.rs` (test only)
- Create: `crates/kmp-mcp/src/serving/adapters/typesafe_judgement_tests.rs` (test only)
- Modify: `crates/kmp-mcp/src/serving/adapters/mod.rs`

**Interfaces:**
- Consumes: everything from Tasks 1–3
- Produces:
  - `TypeSafeJudgement::load(data_dir: &Path, key: Option<String>) -> Result<Option<Arc<dyn JudgementModel>>, String>`
    (called by plan 2 with `optional_env_string(TYPESAFE_API_KEY_ENV)`)
  - `TypeSafeJudgement::new(endpoint: reqwest::Url, model: String, key: TypeSafeApiKey, timeout: Duration) -> Result<Self, String>`
    (`pub(super)`, which is how the tests point it at the fixture)
  - `impl JudgementModel for TypeSafeJudgement`

- [ ] **Step 1: Write the fixture**

`serving/adapters/typesafe_fixture.rs`:

```rust
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::Value;

/// One scripted reply: status, extra headers, body, and a delay before
/// answering.
pub(super) struct Reply {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    pub body: String,
    pub delay: Duration,
}

/// What the fixture received: the raw header block and the JSON body.
pub(super) struct Received {
    pub headers: String,
    pub body: Value,
}

/// Serves the replies in order, one connection each, then disappears. A
/// call beyond the script fails to connect instead of being answered.
pub(super) fn serve(replies: Vec<Reply>) -> (reqwest::Url, JoinHandle<Vec<Received>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback bind");
    let url = reqwest::Url::parse(&format!(
        "http://{}/v1/systemone",
        listener.local_addr().expect("address")
    ))
    .expect("fixture url");
    let handle = std::thread::spawn(move || {
        let mut received = Vec::new();
        for reply in replies {
            let (mut stream, _) = listener.accept().expect("request");
            stream
                .set_read_timeout(Some(Duration::from_secs(20)))
                .expect("timeout");
            let mut data = Vec::new();
            let header_end = loop {
                let mut byte = [0u8; 1];
                stream.read_exact(&mut byte).expect("headers");
                data.push(byte[0]);
                if data.ends_with(b"\r\n\r\n") {
                    break data.len();
                }
            };
            let headers = String::from_utf8_lossy(&data).to_string();
            let length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|n| n.trim().parse::<usize>().expect("length"))
                })
                .expect("content length");
            data.resize(header_end + length, 0);
            stream.read_exact(&mut data[header_end..]).expect("body");
            received.push(Received {
                headers,
                body: serde_json::from_slice(&data[header_end..]).expect("JSON body"),
            });
            std::thread::sleep(reply.delay);
            let extra = reply
                .headers
                .iter()
                .map(|(name, value)| format!("{name}: {value}\r\n"))
                .collect::<String>();
            let _ = write!(
                stream,
                "HTTP/1.1 {} X\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                reply.status,
                reply.body.len(),
                reply.body
            );
        }
        received
    });
    (url, handle)
}
```

- [ ] **Step 2: Write the failing adapter tests**

`serving/adapters/typesafe_judgement_tests.rs`:

```rust
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
    assert_eq!(response.answers["animal"], JudgementAnswer::Noul { yes: 0.99 });
    assert_eq!((response.input_tokens, response.requests), (280, 1));
    assert!(
        received[0]
            .headers
            .to_ascii_lowercase()
            .contains(&format!("authorization: bearer {KEY}"))
    );
    assert_eq!(received[0].body["model"], "jev-1.13.0");
    assert_eq!(received[0].body["questions"]["animal"]["type"], "noul");
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
    let error = TypeSafeJudgement::load(dir.path(), None)
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
```

In `serving/adapters/mod.rs` add:

```rust
#[allow(dead_code)] // consumed by kmp_curate (jev-curate plan 2)
mod typesafe_judgement;
#[cfg(test)]
mod typesafe_fixture;
#[cfg(test)]
mod typesafe_judgement_tests;
```

Create `typesafe_judgement.rs` with the struct and `todo!()` bodies (Step 4
fills them in) so the tests compile and fail.

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p kmp-mcp --lib typesafe_judgement_tests`
Expected: FAIL with `not yet implemented`.

- [ ] **Step 4: Implement the adapter**

`serving/adapters/typesafe_judgement.rs`:

```rust
use std::{collections::BTreeMap, path::Path, pin::Pin, sync::Arc, time::Duration};

use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, RETRY_AFTER};

use super::typesafe_api_key::TypeSafeApiKey;
use super::typesafe_batches::{REQUEST_BYTES, typesafe_batches};
use super::typesafe_config::TypeSafeConfig;
use super::typesafe_request_body::typesafe_request_body;
use super::typesafe_wire_response::TypeSafeWireResponse;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::judgement_response::JudgementResponse;
use crate::serving::ports::judgement_model::JudgementModel;

const MAX_RETRIES: u32 = 2;
const MAX_RETRY_WAIT_SECS: u64 = 5;

/// TypeSafe Jev behind an explicit per-store opt-in. Answers are validated
/// against the questions sent; failures carry no key and no provider body.
#[derive(Debug)]
pub(super) struct TypeSafeJudgement {
    endpoint: reqwest::Url,
    model: String,
    key: TypeSafeApiKey,
    client: reqwest::Client,
}

impl TypeSafeJudgement {
    pub(super) fn load(
        data_dir: &Path,
        key: Option<String>,
    ) -> Result<Option<Arc<dyn JudgementModel>>, String> {
        let bytes = match std::fs::read(data_dir.join("typesafe.json")) {
            Ok(bytes) if bytes.len() <= 8192 => bytes,
            Ok(_) => return Err("TypeSafe configuration exceeds 8192 bytes".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("cannot read TypeSafe configuration".into()),
        };
        let config: TypeSafeConfig =
            serde_json::from_slice(&bytes).map_err(|_| "invalid TypeSafe configuration")?;
        let (endpoint, timeout) = config.validate()?;
        let key = TypeSafeApiKey::from_env(key)?;
        Ok(Some(Arc::new(Self::new(
            endpoint,
            config.model,
            key,
            timeout,
        )?)))
    }

    pub(super) fn new(
        endpoint: reqwest::Url,
        model: String,
        key: TypeSafeApiKey,
        timeout: Duration,
    ) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .timeout(timeout)
            .build()
            .map_err(|_| "cannot create TypeSafe client")?;
        Ok(Self {
            endpoint,
            model,
            key,
            client,
        })
    }

    async fn send(&self, body: Vec<u8>) -> Result<TypeSafeWireResponse, String> {
        if body.len() > REQUEST_BYTES {
            return Err("TypeSafe request exceeds 256 KiB".into());
        }
        let mut attempt = 0;
        loop {
            let mut response = self
                .client
                .post(self.endpoint.clone())
                .header(AUTHORIZATION, self.key.header())
                .header(CONTENT_TYPE, "application/json")
                .body(body.clone())
                .send()
                .await
                .map_err(|error| {
                    if error.is_timeout() {
                        "TypeSafe timed out"
                    } else {
                        "TypeSafe unavailable"
                    }
                })?;
            let status = response.status();
            if status == StatusCode::TOO_MANY_REQUESTS && attempt < MAX_RETRIES {
                let wait = response
                    .headers()
                    .get(RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.trim().parse::<u64>().ok())
                    .unwrap_or(1)
                    .min(MAX_RETRY_WAIT_SECS);
                tokio::time::sleep(Duration::from_secs(wait)).await;
                attempt += 1;
                continue;
            }
            match status.as_u16() {
                200..=299 => {}
                401 | 403 => {
                    return Err(format!(
                        "TypeSafe rejected the API key ({})",
                        status.as_u16()
                    ));
                }
                429 => return Err("TypeSafe rate limit persisted after retries".into()),
                code => return Err(format!("TypeSafe returned HTTP {code}")),
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|error| {
                if error.is_timeout() {
                    "TypeSafe timed out"
                } else {
                    "TypeSafe response interrupted"
                }
            })? {
                if bytes.len() + chunk.len() > REQUEST_BYTES {
                    return Err("TypeSafe response exceeds 256 KiB".into());
                }
                bytes.extend_from_slice(&chunk);
            }
            return serde_json::from_slice(&bytes).map_err(|_| "invalid TypeSafe response".into());
        }
    }
}

impl JudgementModel for TypeSafeJudgement {
    fn model(&self) -> &str {
        &self.model
    }

    fn evaluate<'a>(
        &'a self,
        request: &'a JudgementRequest,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<JudgementResponse, String>> + Send + 'a>>
    {
        Box::pin(async move {
            let mut merged = JudgementResponse {
                model: self.model.clone(),
                answers: BTreeMap::new(),
                input_tokens: 0,
                requests: 0,
            };
            for batch in typesafe_batches(request)? {
                let body =
                    serde_json::to_vec(&typesafe_request_body(&self.model, &request.state, &batch))
                        .map_err(|_| "cannot encode TypeSafe request")?;
                let part = self.send(body).await?.into_response(&self.model, &batch)?;
                merged.answers.extend(part.answers);
                merged.input_tokens += part.input_tokens;
                merged.requests += 1;
            }
            Ok(merged)
        })
    }
}
```

`#[derive(Debug)]` is safe because `TypeSafeApiKey`'s `Debug` redacts. Add
this test to `typesafe_judgement_tests.rs` to pin that down:

```rust
#[test]
fn debug_output_never_contains_the_key() {
    let (url, _fixture) = serve(Vec::new());
    let rendered = format!("{:?}", adapter(url, Duration::from_secs(5)));
    assert!(!rendered.contains(KEY));
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p kmp-mcp --lib typesafe_`
Expected: PASS for every `typesafe_*` test. `typesafe_live_*` is ignored.

- [ ] **Step 6: Lint, run the gate and commit**

```bash
cargo fmt -p kmp-mcp && cargo clippy -p kmp-mcp --all-targets -- -D warnings
bash scripts/ci/kmp-mcp-architecture-gate.sh
git add crates/kmp-mcp/src/serving/adapters
git commit -m "feat(mcp): add TypeSafe Jev judgement adapter with bounded retries"
```

The fixture uses `tempfile`, which is already a dev-dependency of `kmp-mcp`.
If the architecture gate counts test files, keep
`typesafe_judgement_tests.rs` under 600 lines.

---

### Task 5: Operator-path check with the real key

**Files:** none changed unless a check fails.

- [ ] **Step 1: Full crate tests**

Run: `cargo test -p kmp-mcp`
Expected: PASS, with no new failures compared with `main`.

- [ ] **Step 2: Live call**

```bash
set -a; . ~/.config/typesafe.env; set +a
cargo test -p kmp-mcp --lib typesafe_live -- --ignored
```

Expected: PASS, with a noul above 0.5 from `jev-1.13.0`. Do not print the
environment, and do not run with `--nocapture` or with `RUST_LOG` at a debug
level.

- [ ] **Step 3: Coverage of the new files**

Run the crate coverage the way `docs/development/coverage-floors.tsv` and
`docs/development/testing.md` describe. Confirm that `kmp-mcp` stays at or
above its floor and that the new `typesafe_*` files are covered by the tests
above.

- [ ] **Step 4: Report**

Record in the session hand-off: the commit SHAs, the test counts, the result
of the live call (model and input tokens), and a note that the branch stays
local until plan 2 consumes the port.
