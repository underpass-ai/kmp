# `kmp_curate`: relation curation with TypeSafe Jev and the calling agent — design

Status: approved design, not implemented. Branch `feat/jev-curate`, cut from
`main` 719ed5b2.

## Goal

Improve the relations of one or several abouts. The calling agent and a remote
judgement model, TypeSafe Jev, each do the part the other does poorly:

- **Jev** triages many pairs cheaply. It suggests a relation type, doubts
  stored relations, and spots contradictions.
- **The agent** reads only the promising items, writes the `why` and the
  evidence, and commits.

Jev never writes a relation and never writes a `why`.

**Division of work, from the model's nature.** Jev answers only closed
questions (`noul`, `choice`, `score`) and cannot produce words. Anything that
is new text is therefore the agent's: a `why`, an evidence sentence, a label
key or value. Jev chooses among options the kernel or the agent put in front
of it, and it judges the text the agent wrote. It never fills a blank.

This keeps the standing rules on relations. Every relation says why in a
checkable sentence and carries evidence. Nothing is invented. Only
`same_event_as` and `same_entity_as` cross abouts, and only with a
`kmp_relate` proposal and explicit proof. The rule
"new relations generated automatically" in `agent-token-optimization.md`
scoped the #544 token track; this verb generates candidates, not relations.

## Decisions

| Decision | Choice |
| --- | --- |
| Scope | Propose missing relations **and** audit stored ones |
| Write authority | Propose and apply in one verb; apply reuses the `kmp_write_memory` planner |
| Candidate pairs | Hybrid: kernel signals first, then a Jev partner choice for facts left without a pair |
| Data leaving the machine | Opt-in per store (`typesafe.json`); without it the verb works without Jev |
| Model | Pinned version (`jev-1.13.0`), never `-latest` |
| Delivery | Three PRs: shared TypeSafe client → `kmp_curate` → Ask re-ranking (`feat/jev-rerank`) |

## Surface

One tool, `kmp_curate`, with two modes. The tool is not read-only because
`apply` writes.

- `mode: "review"` does not write anything. Its arguments are:
  - `about`, and optionally `abouts[]` (the same selection as `kmp_relate`);
  - `dimensions`, `interval`, `budget` and `page`;
  - `max_pairs`: default 12, maximum 40 (result cap), and `page.entries`:
    default 8, maximum 20.
- `mode: "apply"` writes through the planner. Its arguments are:
  - `review_token`, returned by the review;
  - `accepted[]`: items of `{ item_id, why, evidence, confidence, rel? }`,
    where `rel` overrides the suggestion;
  - `idempotency_key`, plus `actor` as in `kmp_write_memory`.

## Review

The review returns three lists and a `review_token` bound to the selection,
the arguments and the content digest of every fact read.

**`missing`**: pairs with no declared relation between them.

1. **Kernel pairs.**
   - Inside an about, a new deterministic generator pairs facts that share
     rare identifiers, entities, summary terms (including bridged terms) or a
     label (dimension kind plus scope id). It reuses the signals of
     `kmp_relate`'s proposer, restricted to one about.
   - Across abouts, it reuses `kmp_relate`'s `proposed` pairs unchanged.
   - Each pair carries `proposed_by`, the signals and a checkable `why` for
     the pairing, not for the relation. The output is reproducible bit for
     bit.
2. **Jev partners.** Facts the kernel left without a pair are sent in one
   request: `state` is the list of the about's facts, and each orphan asks one
   `choice` over the others plus `none`. Pairs from this step carry
   `proposed_by: jev` and no kernel signal, and the agent is told to read them
   with more care.
3. **Typing.** Every pair gets a Jev `choice` over the writer relation types
   plus `none`. Across abouts, the options are only `same_event_as`,
   `same_entity_as` and `none`. Pairs whose top answer is `none` are dropped.
   The rest are ordered by Jev confidence and capped by `max_pairs`.

**`suspect`**: stored relations Jev doubts.

- One `noul` asks whether the `why` and the evidence support that `rel`
  between those two facts.
- One `choice` asks for the best type.
- A relation is flagged when the `noul` is below 0.3, or when the best type
  differs from the stored one with confidence ≥ 0.7.
- **These findings are reported only.** KMP has no way to retract or retype
  a relation; that capability is out of scope.

**`tensions`**: current facts that contradict each other and are not joined
by `contradicts` or `supersedes`. They come out of the typing choice itself,
as `missing` items with `suggested_rel: contradicts`. `contradicts` has to win
against `supersedes`, `updates_state` and `corrects` in the same choice.

The first operator run (2026-09-24, `project:made`) asked a separate `noul`,
"cannot both be true?", and let it override the type. All twelve items came
back as `contradicts`, although most were a candidate PR followed by the same
PR merged. A later status is not a clash, and only a choice among the
alternatives tells them apart. Structural types are not offered either,
because `relations[]` refuses them.

Every item carries `item_id`, `from` and `to` (as ref, about and a 160-character
excerpt; the 10,000-character result cap rules out full texts), the `suggested_rel`, `proposed_by` with the kernel signals, and
`jev: { probabilities, confidence, model }`.

`next_actions` holds a bound `apply` continuation. The agent fills in `why`
and `evidence` for the items it accepts.

## Apply

1. The verb checks `review_token` against the current selection and content.
   Any change returns a conflict with a fresh `review` action.
2. **Pre-write check.** Before anything reaches the planner, Jev judges each
   accepted item again, this time with the `why` and `evidence` the agent
   wrote. Everything goes in one request:
   - a `noul` asking whether `why` and `evidence` support `rel` between the
     two texts;
   - a `choice` for the best type.

   An item is doubted when the `noul` is below 0.3, or when a different type
   has confidence ≥ 0.7. These are the same thresholds as `suspect`. A
   doubted item is not written. It comes back in `doubted[]` with Jev's
   findings, the agent's text unchanged, and a bound `apply` continuation, so
   the agent can correct it or confirm it as it is. Confirming means sending
   `confirm_doubted: true` for that `item_id`. Jev can ask for another look;
   it cannot refuse a write. The findings are frozen by the digest of the
   accepted items, so a continuation never calls Jev again. If Jev is
   unavailable, the check is skipped with a warning and the write proceeds.
3. Each remaining item becomes an entry in the `relations[]` packet that
   `kmp_write_memory` accepts: `from`, `to`, `rel`, `why`, `evidence`,
   `confidence`. Cross-about equivalences carry their relate proposal in
   `read_context.relate_proposals`.
4. The packet goes through **the same path** as `kmp_write_memory`.
   `write_dispatch.rs` is refactored so that one function (compile with
   `build_relation_plan`, attach the receipt context, call `kmp_ingest`, map
   the result) serves both tools. It is not copied. `strict`, the evidence
   requirement, `needs_review` with its neighbourhood and `review_token`, and
   idempotency therefore behave identically. The kernel still does the
   review; a Rich relation returns `needs_review` from `kmp_curate` with the
   same neighbourhood, and Jev's pre-write findings for that item are attached
   to it.
5. Provenance: the planner has no free-form metadata for relations. The
   shared path gains an internal hook that sets `evidence[].metadata` on the
   ingest packet, which the ingest already accepts:
   `curated_with: <model>`, plus `proposed_by` when it is `jev`.
   `kmp_write_memory` never sets it. `actor` stays the caller's identity.
6. `rel` must stay within what the item allows. Across abouts that means the
   two equivalences only. An item not present in the review is rejected.

**Later, as a separate spec.** The same pre-write check could annotate
`needs_review` in `kmp_write_memory` itself. The hook sits in
`write_dispatch.rs`, before `kmp_ingest`, so it covers the embedded and gRPC
backends alike. It would be opt-in per store, advisory only, and decided
after `kmp_curate` has measured how often Jev's doubts are right.

**Later: Jev suggesting a dynamic label for the nodes.** Decided on
2026-09-24: the label goes on the nodes, meaning the memories at both ends of
a relation, not on the relation.

- **When:** a curated pair has no dimension label (dimension kind plus
  `scope_id`) in common.
- **What Jev is asked.** The labels are the dynamic `key=value` labels
  (#507–#514, v0.12.0): N free labels per entry, with the key in
  `dimension` and the value in `scope_id`. The suggestion is therefore one
  `key=value`, chosen in two steps against the catalogue the about already
  has (the `labels[]` that wake lists):
  1. A `choice` for the **key**: the keys in use, plus `new` and `none`.
  2. A `choice` for the **value** within that key: its values in use, plus
     `new`.

  For `new`, the agent proposes the key or the value, because Jev cannot
  produce words. Jev then checks the agent's proposal with one `noul`: does
  `key=value` describe both memories? A low answer comes back as a doubt, the
  same way the pre-write check handles relations. The agent decides. The
  existing rule still applies ("one value, one key per about; the key is
  fixed on first use"), so a value already stored under another key cannot be
  offered under a different one. A new key or value goes through
  `labels_new` and its "does it resemble one we already have?" guard (#510).
- **Both nodes share it.** The suggestion is one label applied to both
  memories, so they end up sharing it. From then on `kmp_relate` reads the
  pair inside a shared scope (its `coordinate` relations and the proposal
  scope), and a later `kmp_curate` review counts that shared label as a
  kernel signal.
- **How it is applied:** the agent applies it with `kmp_relabel`, which
  already guards labels that resemble stored ones. This needs no change to
  the relation model. Today `relations[]` refuses `labels` and `labels_new`
  (`write/relation_planner.rs:45`, `:388`), and it stays that way.
- **Where it would appear:** as a fourth list in `kmp_curate review`,
  `labels`, with one item per pair: `from`, `to`, `suggested_label`, Jev's
  probabilities and confidence, and a bound `kmp_relabel` action that adds the
  same label to both nodes.
- **Status:** not in this plan. It is the next step once `kmp_curate` has
  measured precision. Labels on the relation itself were considered and are
  not pursued.

## Shared TypeSafe client (PR 1)

This generalises the client from `feat/jev-rerank`
(`docs/development/jev-rerank-design.md` there).

- **Port** `JudgementModel`: batched `noul` and `choice` questions over one
  `state`, returning probabilities, confidence, model and usage. Boxed `Send`
  future.
- **Adapter** `TypeSafeJudgement`: `POST https://api.typesafe.ai/v1/systemone`
  with a Bearer token.
  - It reuses the workspace `reqwest` with rustls; there is no new
    dependency.
  - It follows no redirects and uses no proxy. Requests and responses are
    capped at 256 KiB.
  - On `429` it makes at most two retries, honouring `retry-after` capped at
    5 s.
- **Config** `typesafe.json` beside the store, at most 8 KiB,
  `deny_unknown_fields`: `endpoint` (HTTPS, host `api.typesafe.ai`), a pinned
  `model`, and `timeout_ms`. The key is read from `TYPESAFE_API_KEY` only and
  never logged, echoed or shown by `Debug`.
- **Budget.** Requests are split so that `state` plus the longest question
  stays within the provider's 32k tokens and each request within 64k. Fact
  text is cut to 2,000 characters. A 60-pair review is one to three requests.
- **Degradation.** With no config, no key or a failed call, `review` returns
  the kernel pairs without types, and `suspect` and `tensions` come back
  empty. A warning names the cause. The review never fails because of Jev.
  `apply` does not call Jev.

## Testing

- **Domain:**
  - The intra-about generator is reproducible bit for bit.
  - It never pairs facts already joined.
  - Across abouts it yields equivalences only.
- **Contract:**
  - `review_token` binds selection, arguments and content.
  - `apply` rejects unknown items and changed content.
  - Pre-write check: a doubted item is withheld with findings, and is
    written after `confirm_doubted`. A Jev failure writes with a warning. A
    continuation makes no second Jev call.
  - A parity test shows `apply` and `kmp_write_memory` produce the same
    relations, the same `needs_review` and the same replay for the same
    packet.
- **Adapter** tests against a local HTTP fixture: 200, 401, 429 with
  `retry-after`, a malformed body, mismatched answer keys, a timeout, and
  request splitting.
- **Surface gates** for a new tool:
  - `fixtures/contract/tools_list.json`;
  - `distribution/mcpb/manifest.json`;
  - `plugins/kmp/capabilities.json` and `scripts/ci/kmp-capability-contract.py`;
  - `scripts/ci/mcp-registry.sh`;
  - `scripts/ci/kmp-mcp-architecture-gate.sh` (600 lines, one type per file);
  - the coverage floors (≥ 80 % `kmp-mcp`);
  - the guide: `plugins/kmp/guide/verbs/curate.md`, `topics/routing.md`,
    `topics/relations.md`, `AGENT.md` and `skills/kmp-moves/SKILL.md`.
- **Operator path** with a real key: one real about, `review` → the agent
  accepts some items → `apply`, then `kmp_relate` shows the new relations.
  Record the pairs proposed, the pairs accepted and the requests and tokens
  used in `docs/development/curate.md`, the user documentation written with
  the implementation.

## Out of scope

- Retracting or retyping stored relations.
- Transitive closure of equivalences.
- Applying without the agent, above any threshold.
- The remote gRPC backend.
- Per-about allow lists for what may be sent.
