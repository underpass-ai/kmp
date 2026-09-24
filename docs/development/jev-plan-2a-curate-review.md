# `kmp_curate` review — implementation plan (2a of 3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Ship `kmp_curate` with `mode: "review"`. The review reads one or
several abouts the way `kmp_relate` does, proposes missing relations (kernel
pairs within and across abouts, plus Jev partners for orphans), audits
declared relations with Jev, spots undeclared contradictions, and returns a
paged, frozen review bound by `review_token`. Nothing is written. `apply`
comes in plan 2b.

**Architecture:**
- **Kernel reading.** `kmp-proto-mapping` gains one entry point that runs
  the relate reading with the proposer open to same-about pairs.
- **Bounded context.** `kmp-mcp` gains `curate/`, shaped like `lifecycle/`:
  - `domain/`: facts, links, pairs, findings, thresholds;
  - `application/`: a mapper from the relate response, the judgement plan,
    the `ReviewRelations` use case, and a DTO at the boundary.
- **Embedded tool.** `EmbeddedCurateTool` wires the use case to the embedded
  service, the `JudgementModel` from plan 1 and an in-process review cache.
- **Backends.** The tool is embedded-only. gRPC refuses it the way it refuses
  `kmp_summaries_audit`.

**Spec:** `docs/development/jev-curate-design.md` (Review, Surface and Testing
sections). The adapter comes from `docs/development/jev-plan-1-typesafe-client.md`.

## Global Constraints

- Everything in plan 1's Global Constraints still holds: no new
  dependencies, the key comes from env only, one primary type per file, files
  ≤ 600 lines, clippy `-D warnings`.
- Jev only chooses and judges. No field it returns is ever used as text; any
  new text is the agent's (spec: "Division of work").
- Thresholds: a declared relation is suspect when its support `noul` < 0.3,
  or when a different type wins with confidence ≥ 0.7. A contradiction is
  reported when its `noul` ≥ 0.7. A Jev partner is accepted when its choice
  confidence ≥ 0.5.
- Cross-about items may only suggest `same_event_as`, `same_entity_as` or
  `none` (`KnownMemoryRelationType::may_cross_abouts`).
- Output: every result is capped at 10,000 characters
  (`maxResultSizeChars`). Items therefore carry a 160-character excerpt and
  the ref, not the full text. `page.entries` defaults to 8, maximum 20.
  `max_pairs` defaults to 12, maximum 40. These are adjustments to the
  spec's "text verbatim" and "default 60", forced by the cap and recorded
  in the spec in Task 7.
- Text sent to Jev is cut to 2,000 characters per fact. The partner step
  uses 400-character excerpts, runs only for abouts with ≤ 60 current facts,
  and asks about at most 30 orphans.
- Without Jev (no config, no key, or a failed call), the review returns
  kernel pairs without a suggested type (`suggested_rel: null`),
  `suspect: []`, and a warning naming the cause. It never fails because of
  Jev.
- The review is frozen: `review_token` is the SHA-256 of
  `kmp.curate.review.v1`, the relate selection fingerprint and the canonical
  findings. Pages and plan 2b's `apply` read the frozen review from an
  in-process cache (16 entries). An evicted token returns
  `review expired; run a fresh review`.
- Branch `feat/jev-curate`, worktree `~/Documents/ai/kmp-jev-curate`,
  one commit per task: `feat(mcp): …`, `feat(proto-mapping): …`,
  `docs: …`.

## File structure

| File | Responsibility |
| --- | --- |
| `crates/kmp-proto-mapping/src/v1beta1/memory_mapping/pair_scope.rs` | `PairScope`: across abouts only, or any two facts |
| `crates/kmp-proto-mapping/src/v1beta1/memory_mapping/relate_proposals.rs` | `propose_links` takes a `PairScope` |
| `crates/kmp-proto-mapping/src/v1beta1/memory_mapping/relate.rs` | `relate_response_from_result` passes `AcrossAbouts`; new `curate_reading_from_result` passes `Any` and does not page |
| `crates/kmp-mcp/src/curate/mod.rs` | Context root |
| `curate/domain/{curate_fact,declared_link,pair_origin,candidate_pair,jev_verdict,curate_finding,curate_thresholds}.rs` | Domain types, one per file |
| `curate/application/curate_material.rs` | `CurateMaterial`: current facts, declared links, candidate pairs |
| `curate/application/mappers/relate_material_mapper.rs` | `RelateResponse` → `CurateMaterial` |
| `curate/application/judgement_plan.rs` | Builds the Jev requests for partners, typing, suspects and tensions |
| `curate/application/jev_usage.rs` | `JevUsage`: model, requests, input tokens |
| `curate/application/curate_review.rs` | `CurateReview`: the findings, usage, warnings, selection and token |
| `curate/application/use_cases/review_relations.rs` | `ReviewRelations`: the use case |
| `curate/application/dto/curate_review_dto.rs` | `CurateReview` → tool JSON, paged |
| `crates/kmp-mcp/src/serving/adapters/curate_review_cache.rs` | `CurateReviewCache`: frozen reviews by token |
| `crates/kmp-mcp/src/serving/adapters/embedded/curate_tool.rs` | `EmbeddedCurateTool` |
| `crates/kmp-mcp/src/contract/tools/curate.rs` | Tool definition |
| Surface and guide files listed in Task 6 | Gates |

---

### Task 1: Proposer open to same-about pairs, and an unpaged curate reading

**Files:**
- Create: `crates/kmp-proto-mapping/src/v1beta1/memory_mapping/pair_scope.rs`
- Modify: `relate_proposals.rs` (signature and the `if first.about == second.about` line)
- Modify: `relate.rs` (split the body into `relate_response_with`, then add the two public entry points)
- Modify: `crates/kmp-proto-mapping/src/v1beta1/memory_mapping.rs` (the `mod`/`pub use` lines next to `pub use relate::relate_response_from_result;`)
- Modify: `crates/kmp-proto-mapping/src/v1beta1.rs:23` (re-export `curate_reading_from_result`)

**Interfaces:**
- Produces: `pub fn curate_reading_from_result(result: GetContextResult, query: &RelateMemoryQuery, bridge: &LexicalBridge) -> ProtoMappingResult<RelateResponse>`.
  It is the same response as relate, with two differences: `proposed`
  includes same-about pairs, and nothing is paged, so every fact, declared
  relation, tension and proposal is present.

- [ ] **Step 1: Write the failing test** at the end of the `#[cfg(test)]` module in `relate_proposals.rs`, reusing that module's existing fixture helpers (read them first and match their names):

```rust
#[test]
fn any_scope_pairs_facts_of_one_about_and_across_scope_never_does() {
    let (facts, words) = two_facts_sharing_ticket_in_one_about();
    let morphology = Morphology::default();
    let bridge = LexicalBridge::silent();
    assert!(
        propose_links(&facts, &words, &morphology, &bridge, PairScope::AcrossAbouts).is_empty()
    );
    let within = propose_links(&facts, &words, &morphology, &bridge, PairScope::Any);
    assert_eq!(within.len(), 1);
    assert_eq!(within[0].signals()[0].name(), "identifier");
}
```

`two_facts_sharing_ticket_in_one_about()` builds three `FactWords` in the
about `a`. Two of them mention `#4711` and the third does not, so the
identifier is rare. It also builds matching `RelatedFact::new(ref, "a", vec![], FactState::Current)`.
Add it as a test helper next to the module's existing helpers. If the module
already has a builder for `FactWords`/`RelatedFact`, use it. If
`Morphology::default()` or `LexicalBridge::silent()` do not exist, use the
constructors the existing test at `:211` uses.

- [ ] **Step 2: Run the test to see it fail**

Run: `cargo test -p kmp-proto-mapping any_scope_pairs`
Expected: a compile error, because `PairScope` is not defined and `propose_links` takes 4 arguments.

- [ ] **Step 3: Implement**

`pair_scope.rs`:

```rust
/// Which pairs the proposer may read. `kmp_relate` proposes only across
/// abouts, because a proposal there is a comparison between two owners.
/// Curation also reads inside one about, where a missing relation is as real
/// as a missing equivalence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PairScope {
    AcrossAbouts,
    Any,
}
```

In `relate_proposals.rs`, add the parameter `scope: PairScope` as the last
argument of `propose_links`, and replace:

```rust
            if first.about == second.about {
                continue;
            }
```

with:

```rust
            if scope == PairScope::AcrossAbouts && first.about == second.about {
                continue;
            }
```

Update the existing tests that call `propose_links` so they pass
`PairScope::AcrossAbouts`.

In `relate.rs`:
1. Rename `relate_response_from_result`'s body into
   `fn relate_response_with(result: GetContextResult, query: &RelateMemoryQuery, bridge: &LexicalBridge, scope: PairScope) -> ProtoMappingResult<RelateResponse>`,
   and pass `scope` to `propose_links`.
2. Add the two public entry points:

```rust
pub fn relate_response_from_result(
    result: GetContextResult,
    query: &RelateMemoryQuery,
    bridge: &LexicalBridge,
) -> ProtoMappingResult<RelateResponse> {
    relate_response_with(result, query, bridge, PairScope::AcrossAbouts)
}

/// The whole relate reading for curation: proposals inside one about as well
/// as across abouts, and no page, since curation freezes what it read.
pub fn curate_reading_from_result(
    result: GetContextResult,
    query: &RelateMemoryQuery,
    bridge: &LexicalBridge,
) -> ProtoMappingResult<RelateResponse> {
    let mut whole = query.clone();
    whole.page.entries = Some(usize::MAX);
    whole.page.cursor = None;
    relate_response_with(result, &whole, bridge, PairScope::Any)
}
```

In `memory_mapping.rs`, add `mod pair_scope;` and extend
`pub use relate::{curate_reading_from_result, relate_response_from_result};`.
In `v1beta1.rs:23`, add `curate_reading_from_result` to the re-export list.
The pair loop and `end = offset.saturating_add(...)` already saturate, so
`usize::MAX` is safe.

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p kmp-proto-mapping relate`
Expected: PASS, including every existing relate test (the relate output is unchanged).

- [ ] **Step 5: Relate parity is unchanged**

Run: `cargo test -p kmp-mcp --test stdio_binary relate_` and `cargo test -p kmp-mcp --test tool_surface_parity`
Expected: PASS without blessing.

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt --all && cargo clippy -p kmp-proto-mapping -p kmp-mcp --all-targets -- -D warnings
git add crates/kmp-proto-mapping
git commit -m "feat(proto-mapping): open relate proposals to same-about pairs for curation"
```

---

### Task 2: Curate domain and material mapper

**Files:**
- Create: `crates/kmp-mcp/src/curate/mod.rs`, `curate/domain/mod.rs`, the seven domain files, `curate/application/mod.rs`, `curate/application/curate_material.rs`, `curate/application/mappers/mod.rs`, `curate/application/mappers/relate_material_mapper.rs`
- Modify: `crates/kmp-mcp/src/lib.rs` (add `mod curate;` after `mod contract;`)

**Interfaces:**
- Produces:
  - `CurateFact { reference: String, about: String, text: String }`, current facts only.
  - `DeclaredLink { from, to, rel, why, evidence: String }`.
  - `PairOrigin::{Kernel { signals: Vec<String>, why: String }, Jev}` with `fn name(&self) -> &'static str` returning `"kernel"` or `"jev"`.
  - `CandidatePair { from: String, to: String, origin: PairOrigin, crosses_abouts: bool }`.
  - `JevVerdict { choice: String, probabilities: BTreeMap<String, f64>, confidence: f64 }`.
  - `CurateFinding::{Missing { pair: CandidatePair, suggested_rel: Option<String>, verdict: Option<JevVerdict> }, Suspect { link: DeclaredLink, support: f64, best: JevVerdict }}`.
  - `curate_thresholds`: `pub(crate) const DOUBT_BELOW: f64 = 0.3; RETYPE_AT: f64 = 0.7; CONTRADICTION_AT: f64 = 0.7; PARTNER_AT: f64 = 0.5; NONE: &str = "none";`
  - `CurateMaterial { facts: Vec<CurateFact>, declared: Vec<DeclaredLink>, pairs: Vec<CandidatePair>, selection: String }`, with `fn fact(&self, reference: &str) -> Option<&CurateFact>` and `fn orphans(&self) -> Vec<&CurateFact>`.
  - `relate_material(response: &RelateResponse) -> CurateMaterial`.

- [ ] **Step 1: Write the domain types**

`curate/mod.rs`:

```rust
//! `kmp_curate`: relations of one or several abouts, curated by the calling
//! agent with TypeSafe Jev as a cheap second reader. Jev chooses among
//! options and judges text; it never writes a relation or a word of one.

pub(crate) mod application;
pub(crate) mod domain;
```

`curate/domain/mod.rs` declares `pub(crate) mod` for each of the seven files.
Each file holds one type, with `#[derive(Clone, Debug, PartialEq)]` (add
`Eq` where no `f64` is involved) and `pub` fields, following the Interfaces
block above. `PairOrigin::name`:

```rust
impl PairOrigin {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Kernel { .. } => "kernel",
            Self::Jev => "jev",
        }
    }
}
```

- [ ] **Step 2: Write the failing mapper test**

`curate/application/mappers/relate_material_mapper.rs`, with the tests first
and the function returning `todo!()`:

```rust
use kmp_proto::v1beta1::{FactState, RelateResponse};

use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::domain::candidate_pair::CandidatePair;
use crate::curate::domain::curate_fact::CurateFact;
use crate::curate::domain::declared_link::DeclaredLink;
use crate::curate::domain::pair_origin::PairOrigin;

/// What curation reads from a relate reading: the current facts, what is
/// declared between them, and the kernel's proposed pairs that nothing
/// declares yet. Superseded and expired facts are history, not candidates.
pub(crate) fn relate_material(response: &RelateResponse) -> CurateMaterial {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kmp_proto::v1beta1::{MemoryRelation, ProposedLink, RelatedFact};

    fn fact(reference: &str, about: &str, state: FactState) -> RelatedFact {
        RelatedFact {
            r#ref: reference.into(),
            about: about.into(),
            text: format!("text of {reference}"),
            state: state as i32,
            ..RelatedFact::default()
        }
    }

    fn proposal(from: &str, to: &str) -> ProposedLink {
        ProposedLink {
            from: from.into(),
            to: to.into(),
            proposed_by: vec!["identifier".into()],
            why: "both cite #4711".into(),
            ..ProposedLink::default()
        }
    }

    #[test]
    fn only_current_undeclared_pairs_become_candidates() {
        let response = RelateResponse {
            facts: vec![
                fact("a1", "a", FactState::Current),
                fact("a2", "a", FactState::Current),
                fact("a3", "a", FactState::Superseded),
                fact("b1", "b", FactState::Current),
            ],
            declared: vec![MemoryRelation {
                source_ref: "a2".into(),
                target_ref: "a1".into(),
                rel: "supports".into(),
                why: "w".into(),
                evidence: "e".into(),
                ..MemoryRelation::default()
            }],
            proposed: vec![
                proposal("a1", "a2"),
                proposal("a1", "a3"),
                proposal("a1", "b1"),
            ],
            selection_fingerprint: "fp".into(),
            ..RelateResponse::default()
        };
        let material = relate_material(&response);
        assert_eq!(material.selection, "fp");
        assert_eq!(material.facts.len(), 3);
        assert_eq!(material.declared.len(), 1);
        assert_eq!(material.pairs.len(), 1, "a1-a2 declared, a3 superseded");
        let pair = &material.pairs[0];
        assert_eq!((pair.from.as_str(), pair.to.as_str()), ("a1", "b1"));
        assert!(pair.crosses_abouts);
        assert_eq!(pair.origin.name(), "kernel");
        assert_eq!(
            material.orphans().iter().map(|f| f.reference.as_str()).collect::<Vec<_>>(),
            Vec::<&str>::new(),
            "a1 and a2 are declared, b1 is paired"
        );
    }
}
```

- [ ] **Step 3: Implement `CurateMaterial` and the mapper**

`curate/application/curate_material.rs`:

```rust
use std::collections::BTreeSet;

use crate::curate::domain::candidate_pair::CandidatePair;
use crate::curate::domain::curate_fact::CurateFact;
use crate::curate::domain::declared_link::DeclaredLink;

/// One frozen reading to curate: current facts, declared links, candidate
/// pairs, and the fingerprint of the relate selection they came from.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CurateMaterial {
    pub facts: Vec<CurateFact>,
    pub declared: Vec<DeclaredLink>,
    pub pairs: Vec<CandidatePair>,
    pub selection: String,
}

impl CurateMaterial {
    pub(crate) fn fact(&self, reference: &str) -> Option<&CurateFact> {
        self.facts.iter().find(|fact| fact.reference == reference)
    }

    /// Current facts that neither a declared link nor a candidate pair
    /// touches: the ones only a reader of meaning can still pair.
    pub(crate) fn orphans(&self) -> Vec<&CurateFact> {
        let touched = self
            .declared
            .iter()
            .flat_map(|link| [link.from.as_str(), link.to.as_str()])
            .chain(
                self.pairs
                    .iter()
                    .flat_map(|pair| [pair.from.as_str(), pair.to.as_str()]),
            )
            .collect::<BTreeSet<_>>();
        self.facts
            .iter()
            .filter(|fact| !touched.contains(fact.reference.as_str()))
            .collect()
    }
}
```

The mapper body:

```rust
    let facts = response
        .facts
        .iter()
        .filter(|fact| fact.state == FactState::Current as i32)
        .map(|fact| CurateFact {
            reference: fact.r#ref.clone(),
            about: fact.about.clone(),
            text: fact.text.clone(),
        })
        .collect::<Vec<_>>();
    let about_of = |reference: &str| {
        facts
            .iter()
            .find(|fact| fact.reference == reference)
            .map(|fact| fact.about.clone())
    };
    let declared = response
        .declared
        .iter()
        .map(|relation| DeclaredLink {
            from: relation.source_ref.clone(),
            to: relation.target_ref.clone(),
            rel: relation.rel.clone(),
            why: relation.why.clone(),
            evidence: relation.evidence.clone(),
        })
        .collect::<Vec<_>>();
    let joined = |left: &str, right: &str| {
        declared.iter().any(|link| {
            (link.from == left && link.to == right) || (link.from == right && link.to == left)
        })
    };
    let pairs = response
        .proposed
        .iter()
        .filter_map(|proposal| {
            let from_about = about_of(&proposal.from)?;
            let to_about = about_of(&proposal.to)?;
            (!joined(&proposal.from, &proposal.to)).then(|| CandidatePair {
                from: proposal.from.clone(),
                to: proposal.to.clone(),
                origin: PairOrigin::Kernel {
                    signals: proposal.proposed_by.clone(),
                    why: proposal.why.clone(),
                },
                crosses_abouts: from_about != to_about,
            })
        })
        .collect();
    CurateMaterial {
        facts,
        declared,
        pairs,
        selection: response.selection_fingerprint.clone(),
    }
```

Add `pub(crate) mod curate_material; pub(crate) mod mappers;` to
`curate/application/mod.rs`, and `pub(crate) mod relate_material_mapper;` to
`mappers/mod.rs`. Add `mod curate;` to `lib.rs`, with
`#[allow(dead_code)] // consumed by EmbeddedCurateTool (Task 4)` on the
`mod curate;` line until Task 4 wires it.

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p kmp-mcp --lib curate::`
Expected: PASS (1 test).

- [ ] **Step 5: Lint, run the gate and commit**

```bash
cargo fmt -p kmp-mcp && cargo clippy -p kmp-mcp --all-targets -- -D warnings
bash scripts/ci/kmp-mcp-architecture-gate.sh
git add crates/kmp-mcp/src
git commit -m "feat(mcp): add curate domain and relate material mapper"
```

---

### Task 3: Judgement plan and the `ReviewRelations` use case

**Files:**
- Create: `curate/application/judgement_plan.rs`, `curate/application/jev_usage.rs`, `curate/application/curate_review.rs`, `curate/application/use_cases/mod.rs`, `curate/application/use_cases/review_relations.rs`
- Test: inline, with a scripted `JudgementModel`

**Interfaces:**
- Consumes: `JudgementModel`, `JudgementRequest`, `JudgementQuestion`, `JudgementAnswer` and `JudgementResponse` (plan 1), plus the Task 2 types.
- Produces:
  - `JevUsage { model: String, requests: usize, input_tokens: u64 }`.
  - `CurateReview { findings: Vec<CurateFinding>, jev: Option<JevUsage>, warnings: Vec<String>, selection: String }`, with `fn token(&self) -> String`.
  - `ReviewRelations<'a> { judgement: Option<&'a dyn JudgementModel> }`, with `pub(crate) async fn run(&self, material: CurateMaterial, max_pairs: usize) -> CurateReview`.
  - `judgement_plan`:
    - `relation_options(crosses_abouts: bool) -> Vec<String>`
    - `partner_request(facts: &[&CurateFact], orphans: &[&CurateFact]) -> Option<(JudgementRequest, BTreeMap<String, String>)>`
    - `pair_request(material: &CurateMaterial, pairs: &[CandidatePair]) -> JudgementRequest`
    - `suspect_request(material: &CurateMaterial) -> JudgementRequest`
    - `excerpt(text: &str, chars: usize) -> String`

- [ ] **Step 1: Write the judgement plan**

```rust
use std::collections::BTreeMap;

use kmp_domain::KnownMemoryRelationType;
use serde_json::json;

use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::domain::candidate_pair::CandidatePair;
use crate::curate::domain::curate_fact::CurateFact;
use crate::curate::domain::curate_thresholds::NONE;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;

const SENT_CHARS: usize = 2_000;
const PARTNER_CHARS: usize = 400;
const PARTNER_FACTS: usize = 60;
const PARTNER_ORPHANS: usize = 30;

pub(crate) fn excerpt(text: &str, chars: usize) -> String {
    text.chars().take(chars).collect()
}

/// The relation names a writer may declare, plus `none`. Across abouts only
/// the equivalences may be declared.
pub(crate) fn relation_options(crosses_abouts: bool) -> Vec<String> {
    KnownMemoryRelationType::writer_relation_types()
        .iter()
        .filter(|relation| !crosses_abouts || relation.may_cross_abouts())
        .map(|relation| relation.as_str().to_string())
        .chain(std::iter::once(NONE.to_string()))
        .collect()
}

fn text_of<'a>(material: &'a CurateMaterial, reference: &str) -> &'a str {
    material
        .fact(reference)
        .map(|fact| fact.text.as_str())
        .unwrap_or_default()
}

/// One request per about: its facts as the state, one choice per orphan over
/// the others. Keys `f<n>` map back to refs. None when the about is too
/// large to show whole or has nothing to pair.
pub(crate) fn partner_request(
    facts: &[&CurateFact],
    orphans: &[&CurateFact],
) -> Option<(JudgementRequest, BTreeMap<String, String>)> {
    if facts.len() < 2 || facts.len() > PARTNER_FACTS || orphans.is_empty() {
        return None;
    }
    let keys = facts
        .iter()
        .enumerate()
        .map(|(n, fact)| (format!("f{n}"), fact.reference.clone()))
        .collect::<BTreeMap<_, _>>();
    let key_of = |reference: &str| {
        keys.iter()
            .find(|(_, r)| r.as_str() == reference)
            .map(|(k, _)| k.clone())
    };
    let state = json!({ "facts": facts.iter().enumerate()
        .map(|(n, fact)| (format!("f{n}"), json!(excerpt(&fact.text, PARTNER_CHARS))))
        .collect::<serde_json::Map<_, _>>() });
    let mut questions = BTreeMap::new();
    for orphan in orphans.iter().take(PARTNER_ORPHANS) {
        let Some(own) = key_of(&orphan.reference) else {
            continue;
        };
        let options = keys
            .keys()
            .filter(|key| **key != own)
            .cloned()
            .chain(std::iter::once(NONE.to_string()))
            .collect();
        questions.insert(
            own.clone(),
            JudgementQuestion::Choice {
                instructions: json!(format!(
                    "Which fact in `facts` has the most direct relation to `facts.{own}`: \
                     it causes, explains, supports, contradicts, answers or repeats it? \
                     Answer none when no fact does."
                )),
                options,
            },
        );
    }
    Some((JudgementRequest { state, questions }, keys))
}

/// For each pair: its best relation type (`t<n>`) and whether the two facts
/// contradict each other (`c<n>`).
pub(crate) fn pair_request(material: &CurateMaterial, pairs: &[CandidatePair]) -> JudgementRequest {
    let mut questions = BTreeMap::new();
    for (n, pair) in pairs.iter().enumerate() {
        let texts = json!({
            "from": excerpt(text_of(material, &pair.from), SENT_CHARS),
            "to": excerpt(text_of(material, &pair.to), SENT_CHARS),
        });
        let mut typed = texts.clone();
        typed["question"] = json!(
            "Which relation does `from` have to `to`? Answer none when no relation holds."
        );
        questions.insert(
            format!("t{n}"),
            JudgementQuestion::Choice {
                instructions: typed,
                options: relation_options(pair.crosses_abouts),
            },
        );
        let mut clash = texts;
        clash["question"] =
            json!("Do `from` and `to` state things that cannot both be true?");
        questions.insert(format!("c{n}"), JudgementQuestion::Noul { instructions: clash });
    }
    JudgementRequest {
        state: json!("Two memories of one knowledge base, judged pair by pair."),
        questions,
    }
}

/// For each declared link: does its reason hold (`s<n>`), and which type
/// fits best (`b<n>`).
pub(crate) fn suspect_request(material: &CurateMaterial) -> JudgementRequest {
    let mut questions = BTreeMap::new();
    for (n, link) in material.declared.iter().enumerate() {
        let crosses = material.fact(&link.from).map(|f| &f.about)
            != material.fact(&link.to).map(|f| &f.about);
        let base = json!({
            "from": excerpt(text_of(material, &link.from), SENT_CHARS),
            "to": excerpt(text_of(material, &link.to), SENT_CHARS),
            "relation": link.rel,
            "why": link.why,
            "evidence": link.evidence,
        });
        let mut support = base.clone();
        support["question"] =
            json!("Do `why` and `evidence` show that `from` has the relation `relation` to `to`?");
        questions.insert(format!("s{n}"), JudgementQuestion::Noul { instructions: support });
        let mut best = base;
        best["question"] = json!(
            "Which relation does `from` have to `to`? Answer none when no relation holds."
        );
        questions.insert(
            format!("b{n}"),
            JudgementQuestion::Choice {
                instructions: best,
                options: relation_options(crosses),
            },
        );
    }
    JudgementRequest {
        state: json!("Declared relations between memories of one knowledge base, audited one by one."),
        questions,
    }
}
```

- [ ] **Step 2: Write `JevUsage` and `CurateReview`**

`jev_usage.rs`:

```rust
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JevUsage {
    pub model: String,
    pub requests: usize,
    pub input_tokens: u64,
}
```

`curate_review.rs`:

```rust
use sha2::{Digest, Sha256};

use crate::curate::application::jev_usage::JevUsage;
use crate::curate::domain::curate_finding::CurateFinding;

/// A finished review. Its token binds the relate selection and every
/// finding, so a later page or apply reads exactly this review.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CurateReview {
    pub findings: Vec<CurateFinding>,
    pub jev: Option<JevUsage>,
    pub warnings: Vec<String>,
    pub selection: String,
}

impl CurateReview {
    pub(crate) fn token(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"kmp.curate.review.v1\0");
        hasher.update(self.selection.as_bytes());
        hasher.update(format!("{:?}", self.findings).as_bytes());
        format!("{:x}", hasher.finalize())
    }
}
```

`Debug` output is deterministic here: every map is a `BTreeMap` and `f64`
prints the same way each time. Add `sha2` to the imports; it is already a
dependency of `kmp-mcp`.

- [ ] **Step 3: Write the failing use-case tests**

`use_cases/review_relations.rs`, with a scripted model at the bottom of the
test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use crate::curate::domain::{
        candidate_pair::CandidatePair, curate_fact::CurateFact, declared_link::DeclaredLink,
        pair_origin::PairOrigin,
    };
    use crate::serving::judgement_answer::JudgementAnswer;
    use crate::serving::judgement_question::JudgementQuestion;
    use crate::serving::judgement_request::JudgementRequest;
    use crate::serving::judgement_response::JudgementResponse;

    /// Answers every noul with `noul` and every choice with `choice` when
    /// offered (else `none`), at `confidence`.
    struct Scripted {
        noul: f64,
        choice: &'static str,
        confidence: f64,
        calls: Mutex<usize>,
    }

    impl JudgementModel for Scripted {
        fn model(&self) -> &str {
            "jev-test"
        }
        fn evaluate<'a>(
            &'a self,
            request: &'a JudgementRequest,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<JudgementResponse, String>> + Send + 'a>,
        > {
            *self.calls.lock().expect("calls") += 1;
            let answers = request
                .questions
                .iter()
                .map(|(key, question)| {
                    let answer = match question {
                        JudgementQuestion::Noul { .. } => JudgementAnswer::Noul { yes: self.noul },
                        JudgementQuestion::Choice { options, .. } => {
                            let choice = if options.iter().any(|o| o == self.choice) {
                                self.choice
                            } else {
                                "none"
                            };
                            JudgementAnswer::Choice {
                                choice: choice.into(),
                                probabilities: options
                                    .iter()
                                    .map(|o| (o.clone(), if o == choice { self.confidence } else { 0.0 }))
                                    .collect(),
                                confidence: self.confidence,
                            }
                        }
                    };
                    (key.clone(), answer)
                })
                .collect::<BTreeMap<_, _>>();
            Box::pin(async move {
                Ok(JudgementResponse {
                    model: "jev-test".into(),
                    answers,
                    input_tokens: 10,
                    requests: 1,
                })
            })
        }
    }

    fn fact(reference: &str, about: &str) -> CurateFact {
        CurateFact {
            reference: reference.into(),
            about: about.into(),
            text: format!("text {reference}"),
        }
    }

    fn material() -> CurateMaterial {
        CurateMaterial {
            facts: vec![fact("a1", "a"), fact("a2", "a"), fact("a3", "a"), fact("b1", "b")],
            declared: vec![DeclaredLink {
                from: "a3".into(),
                to: "a1".into(),
                rel: "causes".into(),
                why: "w".into(),
                evidence: "e".into(),
            }],
            pairs: vec![CandidatePair {
                from: "a1".into(),
                to: "b1".into(),
                origin: PairOrigin::Kernel { signals: vec!["entity".into()], why: "both name Valkey".into() },
                crosses_abouts: true,
            }],
            selection: "fp".into(),
        }
    }

    #[tokio::test]
    async fn without_jev_kernel_pairs_come_back_untyped_with_a_warning() {
        let review = ReviewRelations { judgement: None }.run(material(), 12).await;
        assert_eq!(review.findings.len(), 1);
        assert!(matches!(
            &review.findings[0],
            CurateFinding::Missing { suggested_rel: None, verdict: None, .. }
        ));
        assert!(review.jev.is_none());
        assert!(review.warnings.iter().any(|w| w.contains("Jev")));
    }

    #[tokio::test]
    async fn jev_types_pairs_finds_partners_and_flags_weak_declarations() {
        let model = Scripted { noul: 0.1, choice: "same_entity_as", confidence: 0.9, calls: Mutex::new(0) };
        let review = ReviewRelations { judgement: Some(&model) }.run(material(), 12).await;
        let missing = review.findings.iter().filter(|f| matches!(f, CurateFinding::Missing { .. })).count();
        let suspect = review.findings.iter().filter(|f| matches!(f, CurateFinding::Suspect { .. })).count();
        assert!(missing >= 1, "the cross-about pair is typed same_entity_as");
        assert_eq!(suspect, 1, "support 0.1 is below 0.3");
        assert!(matches!(
            review.findings.iter().find(|f| matches!(f, CurateFinding::Missing { pair, .. } if pair.crosses_abouts)),
            Some(CurateFinding::Missing { suggested_rel: Some(rel), .. }) if rel == "same_entity_as"
        ));
        assert_eq!(review.jev.as_ref().map(|u| u.model.as_str()), Some("jev-test"));
        assert!(*model.calls.lock().expect("calls") >= 2);
    }

    #[tokio::test]
    async fn a_pair_typed_none_is_dropped_and_max_pairs_caps_missing() {
        let model = Scripted { noul: 0.9, choice: "not-offered", confidence: 0.9, calls: Mutex::new(0) };
        let review = ReviewRelations { judgement: Some(&model) }.run(material(), 12).await;
        assert!(
            !review.findings.iter().any(|f| matches!(f, CurateFinding::Missing { suggested_rel: Some(rel), .. } if rel == "none")),
            "none is never proposed"
        );
        let capped = ReviewRelations { judgement: None }.run(material(), 0).await;
        assert!(capped.findings.is_empty());
    }

    #[test]
    fn the_token_binds_selection_and_findings() {
        let base = CurateReview { findings: vec![], jev: None, warnings: vec![], selection: "fp".into() };
        let other = CurateReview { selection: "fp2".into(), ..base.clone() };
        assert_ne!(base.token(), other.token());
        assert_eq!(base.token(), base.clone().token());
    }
}
```

In the third test, `noul: 0.9` means every pair also comes out as a
contradiction. That is fine: the check is only that `none` never appears as
a suggestion. Choices not offered fall back to `none`.

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test -p kmp-mcp --lib curate::application::use_cases`
Expected: FAIL, because `ReviewRelations` is not defined.

- [ ] **Step 5: Implement the use case**

```rust
use std::collections::BTreeMap;

use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::curate_review::CurateReview;
use crate::curate::application::jev_usage::JevUsage;
use crate::curate::application::judgement_plan::{pair_request, partner_request, suspect_request};
use crate::curate::domain::candidate_pair::CandidatePair;
use crate::curate::domain::curate_finding::CurateFinding;
use crate::curate::domain::curate_thresholds::{
    CONTRADICTION_AT, DOUBT_BELOW, NONE, PARTNER_AT, RETYPE_AT,
};
use crate::curate::domain::jev_verdict::JevVerdict;
use crate::curate::domain::pair_origin::PairOrigin;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::judgement_response::JudgementResponse;
use crate::serving::ports::judgement_model::JudgementModel;

/// Review the relations of a reading: kernel pairs, Jev partners for
/// orphans, a type for every pair, contradictions nobody declared, and
/// declared links whose reason does not hold. Writes nothing.
pub(crate) struct ReviewRelations<'a> {
    pub judgement: Option<&'a dyn JudgementModel>,
}

impl ReviewRelations<'_> {
    pub(crate) async fn run(&self, material: CurateMaterial, max_pairs: usize) -> CurateReview {
        let mut review = CurateReview {
            findings: Vec::new(),
            jev: None,
            warnings: Vec::new(),
            selection: material.selection.clone(),
        };
        let Some(model) = self.judgement else {
            review.warnings.push(
                "Jev is not configured for this store; kernel pairs are returned untyped and declared relations were not audited".into(),
            );
            review.findings = untyped(&material.pairs, max_pairs);
            return review;
        };
        let mut usage = JevUsage { model: model.model().to_string(), requests: 0, input_tokens: 0 };
        let mut ask = async |request: JudgementRequest| -> Option<JudgementResponse> {
            match model.evaluate(&request).await {
                Ok(response) => {
                    usage.requests += response.requests;
                    usage.input_tokens += response.input_tokens;
                    Some(response)
                }
                Err(error) => {
                    review.warnings.push(format!("Jev unavailable; using kernel pairs only: {error}"));
                    None
                }
            }
        };

        let mut pairs = material.pairs.clone();
        for about in abouts_with_orphans(&material) {
            let facts = material.facts.iter().filter(|f| f.about == about).collect::<Vec<_>>();
            let orphans = material.orphans().into_iter().filter(|f| f.about == about).collect::<Vec<_>>();
            let Some((request, keys)) = partner_request(&facts, &orphans) else { continue };
            let Some(response) = ask(request).await else { break };
            for (own, answer) in &response.answers {
                if let JudgementAnswer::Choice { choice, confidence, .. } = answer
                    && choice != NONE
                    && *confidence >= PARTNER_AT
                    && let (Some(from), Some(to)) = (keys.get(own), keys.get(choice))
                {
                    pairs.push(CandidatePair { from: from.clone(), to: to.clone(), origin: PairOrigin::Jev, crosses_abouts: false });
                }
            }
        }

        let typed = if pairs.is_empty() { Some(JudgementResponse::empty(&usage.model)) } else { ask(pair_request(&material, &pairs)).await };
        let Some(typed) = typed else {
            review.findings = untyped(&material.pairs, max_pairs);
            return review;
        };
        let mut missing = Vec::new();
        for (n, pair) in pairs.into_iter().enumerate() {
            let verdict = typed.answers.get(&format!("t{n}")).and_then(verdict_of);
            let clash = match typed.answers.get(&format!("c{n}")) {
                Some(JudgementAnswer::Noul { yes }) => *yes,
                _ => 0.0,
            };
            let (suggested, verdict) = if clash >= CONTRADICTION_AT {
                ("contradicts".to_string(), JevVerdict {
                    choice: "contradicts".into(),
                    probabilities: BTreeMap::from([("contradicts".into(), clash), (NONE.into(), 1.0 - clash)]),
                    confidence: clash,
                })
            } else {
                match verdict {
                    Some(verdict) if verdict.choice != NONE => (verdict.choice.clone(), verdict),
                    _ => continue,
                }
            };
            missing.push(CurateFinding::Missing { pair, suggested_rel: Some(suggested), verdict: Some(verdict) });
        }
        missing.sort_by(|left, right| confidence(right).total_cmp(&confidence(left)));
        missing.truncate(max_pairs);
        review.findings = missing;

        if !material.declared.is_empty()
            && let Some(audit) = ask(suspect_request(&material)).await
        {
            for (n, link) in material.declared.iter().enumerate() {
                let support = match audit.answers.get(&format!("s{n}")) {
                    Some(JudgementAnswer::Noul { yes }) => *yes,
                    _ => continue,
                };
                let Some(best) = audit.answers.get(&format!("b{n}")).and_then(verdict_of) else { continue };
                if support < DOUBT_BELOW || (best.choice != link.rel && best.confidence >= RETYPE_AT) {
                    review.findings.push(CurateFinding::Suspect { link: link.clone(), support, best });
                }
            }
        }
        review.jev = Some(usage);
        review
    }
}

fn verdict_of(answer: &JudgementAnswer) -> Option<JevVerdict> {
    match answer {
        JudgementAnswer::Choice { choice, probabilities, confidence } => Some(JevVerdict {
            choice: choice.clone(),
            probabilities: probabilities.clone(),
            confidence: *confidence,
        }),
        JudgementAnswer::Noul { .. } => None,
    }
}

fn confidence(finding: &CurateFinding) -> f64 {
    match finding {
        CurateFinding::Missing { verdict: Some(verdict), .. } => verdict.confidence,
        _ => 0.0,
    }
}

fn untyped(pairs: &[CandidatePair], max_pairs: usize) -> Vec<CurateFinding> {
    pairs
        .iter()
        .take(max_pairs)
        .cloned()
        .map(|pair| CurateFinding::Missing { pair, suggested_rel: None, verdict: None })
        .collect()
}

fn abouts_with_orphans(material: &CurateMaterial) -> Vec<String> {
    let mut abouts = material.orphans().iter().map(|f| f.about.clone()).collect::<Vec<_>>();
    abouts.sort();
    abouts.dedup();
    abouts
}
```

Add a small constructor to plan 1's `JudgementResponse` (in
`serving/judgement_response.rs`):

```rust
impl JudgementResponse {
    /// No questions asked: nothing answered, nothing spent.
    pub(crate) fn empty(model: &str) -> Self {
        Self { model: model.into(), answers: Default::default(), input_tokens: 0, requests: 0 }
    }
}
```

If the `async` closure capturing `&mut usage` and `&mut review.warnings` does
not compile on Rust 1.97, turn `ask` into a private method
`async fn ask(model, request, usage: &mut JevUsage, warnings: &mut Vec<String>) -> Option<JudgementResponse>`
with the same body. The behaviour is identical.

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p kmp-mcp --lib curate::`
Expected: PASS (5 tests).

- [ ] **Step 7: Lint, run the gate and commit**

```bash
cargo fmt -p kmp-mcp && cargo clippy -p kmp-mcp --all-targets -- -D warnings
bash scripts/ci/kmp-mcp-architecture-gate.sh
git add crates/kmp-mcp/src
git commit -m "feat(mcp): add ReviewRelations use case with Jev typing, partners and audit"
```

---

### Task 4: DTO, review cache, embedded tool and backend wiring

**Files:**
- Create: `curate/application/dto/mod.rs`, `curate/application/dto/curate_review_dto.rs`
- Create: `serving/adapters/curate_review_cache.rs`, `serving/adapters/embedded/curate_tool.rs`
- Modify:
  - `serving/adapters/embedded/mod.rs`: re-export `EmbeddedCurateTool`.
  - `serving/adapters/embedded_backend.rs`: fields `judgement` and `curate_reviews`, loading in `open_with_engine_and_commit_native`, and the `"kmp_curate"` arm.
  - `serving/adapters/grpc/tools.rs`: a refusal arm.
  - `serving/adapters/fixture_backend.rs` and `fixtures/kernel/v1beta1/kmp/curate.response.json`.
  - Remove every `#[allow(dead_code)] // consumed by kmp_curate` from plan 1, and the Task 2 one on `mod curate;`.

**Interfaces:**
- Produces:
  - `review_to_value(review: &CurateReview, material: &CurateMaterial, token: &str, offset: usize, entries: usize) -> Value`
  - `CurateReviewCache::{insert(&self, token: String, entry: (CurateReview, CurateMaterial)), get(&self, token: &str) -> Option<(CurateReview, CurateMaterial)>}` (plan 2b's `apply` reads it)
  - The embedded `"kmp_curate"` tool call

- [ ] **Step 1: Write the failing DTO test and implement it**

The output JSON shape, verbatim:

```json
{
  "summary": "…",
  "review_token": "<64 hex>",
  "missing": [{"item_id": "m0", "from": {"ref": "…", "about": "…", "excerpt": "…"},
               "to": {…}, "suggested_rel": "supports" | null, "proposed_by": "kernel" | "jev",
               "signals": ["identifier"], "pairing_why": "…" | null,
               "jev": {"confidence": 0.9, "top": [["supports", 0.9], ["causes", 0.05], ["none", 0.05]]} | null}],
  "suspect": [{"item_id": "s0", "from": {…}, "to": {…}, "rel": "causes", "support": 0.1,
               "suggested_rel": "supports", "jev": {"confidence": 0.8, "top": […]}}],
  "jev": {"model": "jev-1.13.0", "requests": 2, "input_tokens": 5120} | null,
  "page": {"entries": 8, "total": 11, "next_cursor": "8" | null},
  "next_actions": [],
  "warnings": []
}
```

Items are numbered over the whole review: `m<n>` counts `Missing` findings
and `s<n>` counts `Suspect` findings, both in review order. The page slices
the concatenation `missing ++ suspect`. `top` holds the three highest
probabilities, sorted by value descending and then by name. Excerpts are 160
characters. When `next_cursor` is set, `next_actions` holds one call:
`{"tool": "kmp_curate", "arguments": {"mode": "review", "about": <about>, "review_token": <token>, "page": {"cursor": <next>}}}`.
`summary` reads `"<k> missing and <s> suspect relations in <f> facts; Jev <model> used <r> requests"`,
or `"… ; Jev not used"`.

Tests, inline in the DTO file:
- `ids_are_stable_across_pages`: m/s numbering and the page slice.
- `top_three_are_sorted`.
- `a_last_page_has_no_next_action`.

Build the fixtures from `CurateReview` and `CurateMaterial` literals, as in
Task 3's tests.

- [ ] **Step 2: Write the review cache**

```rust
use std::collections::VecDeque;
use std::sync::Mutex;

use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::curate_review::CurateReview;

const KEPT: usize = 16;

/// Frozen reviews by token, so pages and apply read the review that was
/// shown instead of a new one. Oldest out first.
#[derive(Default)]
pub(crate) struct CurateReviewCache {
    reviews: Mutex<VecDeque<(String, CurateReview, CurateMaterial)>>,
}

impl CurateReviewCache {
    pub(crate) fn insert(&self, token: String, review: CurateReview, material: CurateMaterial) {
        let mut reviews = self.reviews.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        reviews.retain(|(kept, _, _)| *kept != token);
        if reviews.len() >= KEPT {
            reviews.pop_front();
        }
        reviews.push_back((token, review, material));
    }

    pub(crate) fn get(&self, token: &str) -> Option<(CurateReview, CurateMaterial)> {
        let reviews = self.reviews.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        reviews
            .iter()
            .find(|(kept, _, _)| kept == token)
            .map(|(_, review, material)| (review.clone(), material.clone()))
    }
}
```

Test: after inserting 17 entries, the first is gone and the last is present.

- [ ] **Step 3: Write the embedded tool**

```rust
use serde_json::Value;

use super::super::curate_review_cache::CurateReviewCache;
use super::super::embedded_errors::{kernel_error, mapping_error};
use super::read_telemetry::EmbeddedReadTelemetry;
use crate::curate::application::dto::curate_review_dto::review_to_value;
use crate::curate::application::mappers::relate_material_mapper::relate_material;
use crate::curate::application::use_cases::review_relations::ReviewRelations;
use crate::serving::adapters::tool_request_mapping::RelateRequestMapper;
use crate::serving::ports::judgement_model::JudgementModel;
use crate::serving::{ToolError, tool_success_result};
use kmp_embedded::EmbeddedMemoryService;
use kmp_proto_mapping::v1beta1::{LexicalBridge, curate_reading_from_result, relate_query_from_proto};

const DEFAULT_MAX_PAIRS: u64 = 12;
const MAX_PAIRS: u64 = 40;
const DEFAULT_ENTRIES: u64 = 8;
const MAX_ENTRIES: u64 = 20;

/// Review mode of `kmp_curate` on the embedded store. A fresh review reads,
/// judges and freezes. A page with `review_token` reads the frozen review
/// and calls nothing.
pub(crate) struct EmbeddedCurateTool<'a> {
    service: &'a EmbeddedMemoryService,
    telemetry: EmbeddedReadTelemetry<'a>,
    bridge: &'a LexicalBridge,
    judgement: Option<&'a dyn JudgementModel>,
    judgement_warning: Option<&'a str>,
    cache: &'a CurateReviewCache,
}
```

`new(...)` takes these six arguments in this order. Then
`call(&self, arguments: &Value) -> Result<Value, ToolError>`:

1. `mode` must be `"review"`. Anything else is `invalid_argument("kmp_curate mode must be review")`; plan 2b adds `apply`.
2. Read `page.entries` (default 8, capped at 20), `page.cursor` (a decimal offset; otherwise `invalid_argument`) and `max_pairs` (default 12, capped at 40).
3. If `review_token` is present, take it from the cache (if it is gone: `invalid_argument("review expired; run a fresh review")`), then return `tool_success_result(review_to_value(...))` with that token.
4. Otherwise build a relate request from the same arguments, dropping `mode`, `max_pairs`, `review_token` and `page`:
   `RelateRequestMapper::from_arguments(&relate_args)`, then `relate_query_from_proto`, then `service.relate(query.clone())` (with `kernel_error("curate", &about)`), then `self.telemetry.observe("kmp_curate", …)`, then `curate_reading_from_result(result, &query, self.bridge)`.
5. `let material = relate_material(&response); let mut review = ReviewRelations { judgement: self.judgement }.run(material.clone(), max_pairs).await;`
   If `judgement_warning` is set, push it onto `review.warnings` first.
6. `let token = review.token(); self.cache.insert(token.clone(), review.clone(), material.clone());`, then return the value for offset 0.

- [ ] **Step 4: Wire the backend**

In `EmbeddedKernelMcpBackend`, add:

```rust
    judgement: Result<Option<Arc<dyn JudgementModel>>, String>,
    curate_reviews: CurateReviewCache,
```

Load them in `open_with_engine_and_commit_native`:

```rust
            judgement: TypeSafeJudgement::load(
                data_dir,
                optional_env_string(TYPESAFE_API_KEY_ENV),
            ),
            curate_reviews: CurateReviewCache::default(),
```

`TypeSafeJudgement::load` is `pub(super)` in `adapters`, and
`embedded_backend.rs` sits in `adapters`, so it is visible. Import
`optional_env_string` and `TYPESAFE_API_KEY_ENV` from
`crate::serving::environment`.

The dispatch arm:

```rust
                "kmp_curate" => {
                    let (judgement, warning) = match &self.judgement {
                        Ok(Some(model)) => (Some(model.as_ref()), None),
                        Ok(None) => (None, None),
                        Err(error) => (None, Some(error.as_str())),
                    };
                    EmbeddedCurateTool::new(
                        &service,
                        telemetry,
                        &self.lexical_bridge,
                        judgement,
                        warning,
                        &self.curate_reviews,
                    )
                    .call(arguments)
                    .await
                }
```

Prefix the warning as `"Jev disabled: {error}"` inside the tool.

In gRPC `tools.rs`, next to the `kmp_summaries_audit` refusal:

```rust
        // Curation reads Jev's opt-in and key beside the local store; a live
        // gRPC kernel has neither.
        "kmp_curate" => Err(ToolError::unavailable(
            "kmp_curate runs against the embedded store, where its TypeSafe opt-in lives; \
             live gRPC mode does not serve it",
        )),
```

For the fixture backend, add the constant and an arm that requires
`["about"]`, copying `kmp_relate`. Create
`fixtures/kernel/v1beta1/kmp/curate.response.json` as a
`{"structuredContent": …}` document with the same envelope as
`relate.response.json` (copy its outer shape), holding one `missing` item,
`suspect: []`, `jev: null` and one warning, all matching the DTO shape above.

Remove the plan-1 `#[allow(dead_code)]` attributes (`serving/mod.rs`,
`serving/ports/mod.rs`, `serving/adapters/mod.rs`, `environment.rs`) and the
Task 2 one. Then run clippy. Keep any attribute clippy still needs, for
example if `REQUEST_BYTES` is used only by the adapter, and record why in the
Deviations section.

- [ ] **Step 5: Integration test through the MCP boundary without Jev**

Create `crates/kmp-mcp/tests/curate_review.rs`. Copy the `call`/`ingest`
helpers from `tests/semantic_retrieval.rs` (lines 11–33), then:

```rust
#[tokio::test]
async fn review_without_jev_lists_kernel_pairs_and_freezes_its_pages() {
    let dir = tempfile::tempdir().expect("dir");
    let server = kmp_mcp::KernelMcpServer::embedded(dir.path()).expect("server");
    for about in ["service:alpha", "service:beta"] {
        call(&server, 1, "kmp_ingest", json!({"about": about, "idempotency_key": format!("seed:{about}"),
            "memory": {"dimensions": [], "entries": [
                {"id": format!("{about}:e1"), "kind": "observation", "text": "Ticket #4711 moved the Valkey cache to cluster mode."},
                {"id": format!("{about}:e2"), "kind": "observation", "text": "Latency dropped after ticket #4711 shipped."}
            ], "relations": [], "evidence": []}})).await;
    }
    let first = call(&server, 2, "kmp_curate", json!({"mode": "review", "about": "service:alpha",
        "dimensions": {"scope": "abouts", "abouts": ["service:alpha", "service:beta"]},
        "page": {"entries": 1}})).await;
    assert!(first["jev"].is_null());
    assert!(first["warnings"].to_string().contains("Jev"));
    assert!(!first["missing"].as_array().expect("missing").is_empty());
    assert!(first["missing"][0]["suggested_rel"].is_null());
    let token = first["review_token"].as_str().expect("token").to_string();
    let cursor = first["page"]["next_cursor"].as_str().expect("more pages").to_string();
    let second = call(&server, 3, "kmp_curate", json!({"mode": "review", "about": "service:alpha",
        "review_token": token, "page": {"entries": 1, "cursor": cursor}})).await;
    assert_eq!(second["review_token"], first["review_token"]);
    assert_ne!(second["missing"], first["missing"]);
}
```

Adjust the ingest payload to whatever `kmp_ingest` requires. The seed must
produce at least two kernel pairs: `#4711` is shared by all four facts, so
the identifier is not rare. The proper name `Valkey` in two facts, or the
identifier in fewer than all facts, gives the needed signals. If the
assertions depend on signal thresholds, make the texts differ so that
exactly two facts share `#4711` in each about and one cross-about pair
shares `Valkey`. Run the test and iterate on the seed text until it holds
for the documented reasons, never by loosening the assertions.

- [ ] **Step 6: Run the tests**

Run: `cargo test -p kmp-mcp --lib curate:: && cargo test -p kmp-mcp --test curate_review`
Expected: PASS. This test cannot pass before Task 5, because an unadvertised
tool is rejected by argument validation. If so, run Task 5 Steps 1–3 first,
then come back.

- [ ] **Step 7: Lint, run the gate and commit**

```bash
cargo fmt -p kmp-mcp && cargo clippy -p kmp-mcp --all-targets -- -D warnings
bash scripts/ci/kmp-mcp-architecture-gate.sh
git add crates/kmp-mcp
git commit -m "feat(mcp): serve kmp_curate review on the embedded store"
```

---

### Task 5: Tool contract and surface gates

Follow `docs/development/agent-surface.md` lines 31–59 ("Añadir un verbo"),
which the repo keeps for exactly this. The concrete edits:

- [ ] **Step 1: Contract.**
  - Create `contract/tools/curate.rs` by copying `relabel.rs`'s structure.
    `tool_definition_with_output("kmp_curate", false, <description>, input, output)`.
  - Input: root `additionalProperties: false`, with:
    - `required: ["mode", "about"]`
    - `mode: {"enum": ["review"]}`
    - `about`: the same helper as relate
    - `dimensions`: `dimensions_schema()`
    - `interval`: `interval_schema()`
    - `axis`: `recall_axis_schema()`
    - `max_pairs`: integer 1–40
    - `review_token`: a string matching `^[0-9a-f]{64}$`
    - `page`: `page_schema("…")`
  - Output: `output_object` with a described field for every key of the DTO.
  - Description: say what review returns, that Jev only chooses and judges,
    that the agent writes every `why`, that nothing is written in review
    mode, that it is opt-in per store through `typesafe.json` and
    `TYPESAFE_API_KEY`, and that it is embedded only.
- [ ] **Step 2: Registration.**
  - Add `pub(crate) mod curate;` to `contract/tools/mod.rs`.
  - In `contract/registry.rs`, add the import, then `curate::definition(),` after `summaries_audit`, and add `kmp_curate` to the actor-optional match at `:72-74`.
  - Add `kmp_curate` to the `serving/call_guidance.rs:56` list.
- [ ] **Step 3: Error help.** Add `"kmp_curate" => ("verb:curate", "example:curate"),` to `serving/tool_error_help.rs` `route`.
- [ ] **Step 4: Surface tests.**
  - `src/contract/surface_audit.rs:130`: 15 becomes 16. Shift the positional indices after `summaries_audit` (for example `tools[11]` → `tools[12]` at `:202`).
  - `tests/tool_surface_parity.rs`: add a `("kmp_curate", json!({"mode": "review", "about": <the seeded about used by the relate entry>}))` entry to `calls()`, and change `:917` from 19 to 20.
  - Bless with `KMP_BLESS_TOOL_SURFACE=1 cargo test -p kmp-mcp --test tool_surface_parity` and review the diff of `fixtures/contract/**`.
- [ ] **Step 5: Manifests and contracts.**
  - Add `{"name": "kmp_curate", "description": …}` to `distribution/mcpb/manifest.json`.
  - Add `"kmp_curate"` to `plugins/kmp/capabilities.json` `mcp_tools`, and to `scripts/ci/kmp-capability-contract.py` `EXPECTED_TOOLS`.
  - Add `("kmp_curate", "0.20.0")` to `src/lifecycle/domain/tool_surface_history.rs` `TOOLS_ADDED_LATER`. First check the workspace version in `Cargo.toml`: after a rebase onto `main` at 0.20.1, use the version this lands in, which is the next unreleased one.
- [ ] **Step 6: HTTP authorization.** In `crates/kmp-mcp-http/src/authorization.rs:31`, add `kmp_curate` to the `WRITE_SCOPE` arm. It writes in 2b and must not change scope later. Then run `authorize_about` for it.
- [ ] **Step 7: Guide.** Give it its own topic, following `kmp_condense`:
  - an `editorial.json` entry `verb:curate` with `text_file: "verbs/curate.md"`;
  - `cards/curate.md` and `examples/curate-*.md`;
  - the `follows` relations;
  - `guidance/guide_scheme.rs` `TOPICS` from 9 to 10;
  - the `topic` enum in `contract/tools/guide.rs:14`;
  - `crates/kmp-release/src/application/mappers/guide_request_mapper.rs` `tool_verb`: `"kmp_curate" => Ok("verb:curate")`;
  - a routing row in `plugins/kmp/guide/topics/routing.md`.

  `verbs/curate.md` teaches:
  1. When to curate: after a batch of writes, or before relying on an about's relations.
  2. How to read `missing` and `suspect`.
  3. That the agent writes every `why` and `evidence`, and that Jev's type is a suggestion.
  4. How to page with `review_token`.
  5. The opt-in: `typesafe.json` beside the store, `TYPESAFE_API_KEY`, and what leaves the machine.

  Then regenerate with `cargo build -p kmp-mcp && cargo run --locked --quiet -p kmp-release -- guide assets write --binary target/debug/kmp-mcp`, and re-bless `kmp_guide.json` with the Step 4 command.
- [ ] **Step 8: Hand-kept docs.**
  - `plugins/kmp/skills/kmp-moves/SKILL.md`: the tool list.
  - `README.md:160-176`: the tool table.
  - `crates/kmp-mcp/README.md`: the "exposes" list and the gRPC table row "MCP-local; unavailable in live gRPC mode".
  - `CHANGELOG.md`: an `### Added` entry under `## [Unreleased]`.
- [ ] **Step 9: Run every gate.**

```bash
cargo test -p kmp-mcp --test tool_surface_parity
cargo test -p kmp-mcp --test progressive_guidance
cargo test -p kmp-mcp --lib surface_audit
python3 scripts/ci/kmp-capability-contract.py
bash scripts/ci/mcp-registry.sh
bash scripts/ci/kmp-mcp-architecture-gate.sh
cargo test -p kmp-release --test plugin_package_contract
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: all PASS.

- [ ] **Step 10: Commit.** `git commit -m "feat(mcp): advertise kmp_curate review with guide and surface gates"`

---

### Task 6: Operator-path verification with the real key

- [ ] **Step 1:** `cargo test -p kmp-mcp` and `cargo test --workspace --locked`. Expected: nothing fails beyond the two `stdio_binary` snapshot tests that already fail on `main` on this machine (the store `saved` rule). Report them.
- [ ] **Step 2:** Coverage of the new files with `cargo llvm-cov -p kmp-mcp --lib --json --summary-only`. The new `curate/**` files must be ≥ 80 %.
- [ ] **Step 3:** Live review on a real store:
  1. Build `target/debug/kmp-mcp`.
  2. Make a disposable copy of the store (`~/.local/share/kmp/default`) into the scratchpad, and write `typesafe.json` there with `{"endpoint": "https://api.typesafe.ai/v1/systemone", "model": "jev-1.13.0", "timeout_ms": 20000}`.
  3. Load the key with `set -a; . ~/.config/typesafe.env; set +a`, and set `KMP_MCP_DATA_DIR` to the copy.
  4. Call `kmp_curate {"mode": "review", "about": <an about with relations>}` over stdio JSON-RPC.
  5. Record the number of missing and suspect items, the Jev requests and input tokens, and three sample items with their suggested types, and judge by reading them whether they make sense.

  Never print the key. Never write to the original store.
- [ ] **Step 4:** Record the result in the spec's Testing section and in the memory note. The branch stays local until 2b.

## Deviations recorded during execution


- Task 4: `review_to_value` also takes `about`, because the continuation
  action it builds must name it.
- Task 4: `context_id` is removed from the arguments before the relate
  mapping, along with `mode`, `max_pairs`, `review_token` and `page`.
- Task 4: the integration test moves after Task 5, because an unadvertised
  tool is refused by argument validation.
- Task 5: the parity test marks `review_token` volatile, because it digests
  the relate selection fingerprint, which covers ingestion clocks. Its
  findings stay pinned. `info_report` and `persistent_guidance` count 16 tools
  and 10 guide topics. `budget` is accepted, as in relate.
- Task 6 (live run on a copy of the store, `project:made`, 307 facts): the
  separate contradiction `noul` made all 12 items `contradicts`, because it
  read status updates as clashes. It was removed. `contradicts` now competes
  inside the typing choice, and structural types are no longer offered. The
  second run returned `updates_state`, `supersedes`, `confirms_selection`,
  `violates_constraint` and three `contradicts` at lower confidence:
  3 requests, 103,564 input tokens, 25 s. It found 0 suspect relations. The
  partner step does not run for abouts with more than 60 facts.
- Task 6: a stored type that is not among the offered options (legacy or
  kernel-written, such as `causes`) is judged on support alone, because Jev's
  choice can never match it. A test for the audit-only path covers
  `JudgementResponse::empty`. Coverage of the new code (curate context and
  TypeSafe adapter): 95.2% of 1,620 lines.
