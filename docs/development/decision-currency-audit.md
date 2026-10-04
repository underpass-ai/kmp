# Decision currency: skill/guide audit and agent evaluation

Issues: [#900](https://github.com/underpass-ai/kmp/issues/900) (last recorded
decision vs verified current state) and
[#901](https://github.com/underpass-ai/kmp/issues/901) (is the installed skill
and guide current?). Run on 2026-10-04 against KMP 0.25.0 (`5da095ee`).

## Verdict

- **#901.** The released `kmp-memory` skill and `guide:kmp-agent` are
  installed identically in Claude Code and Codex, match the 0.25.0 tree byte
  for byte and match the live tool contracts on every audited route. No
  released instruction is stale. One gap was not stale but *silent*: nothing
  told the agent how to word a decision that has no later verification,
  replacement or end. A legacy KMP block in the user's `~/.codex/AGENTS.md`,
  written by a pre-0.5.2 installer, is stale and outside the released assets.
- **#900.** KMP's tool evidence is correct for all five cases: `superseded`,
  `conflicts` and `expired` are populated exactly where they should be, the
  open-ended decision has none, and historical reads do not leak later
  information. Agent wording was correct for the historical read, replacement,
  conflict and expiry cases. For the open-ended case it depended on phrasing:
  “Is CSV still the decision?” drew “Yes. … KMP still records CSV as the
  decision” on both the skill route and the no-skill control, without the
  observation date or the missing-verification caveat.
- **Cause and correction.** The guidance was ambiguous by omission, not wrong,
  and the kernel's evidence was complete. One sentence was added to the two
  cards every session read (`card:wake`, `card:ask`), plus a short section in
  the lifecycle topic and a replayed lesson. On a fresh store with the
  revised guide, all three re-run questions produced the observation date
  and said that KMP does not verify the present. Two pass; the “Is CSV still
  the decision?” phrasing still opens with “Yes”, so it is graded PARTIAL. There is no kernel change, age rule or automatic
  supersession.

## Isolation

Every binary, test and agent run used scratch directories:
`KMP_MCP_DATA_DIR=<scratch>/eval|eval-ctl|eval2|noguide|store`,
`XDG_DATA_HOME`/`XDG_CONFIG_HOME` under scratch and, for cargo, `HOME` under
scratch (#910). The Codex runs overrode the plugin's MCP server
(`-c mcp_servers.kmp.command=…` and `-c mcp_servers.kmp.env={…}`) because the
plugin manifest forwards only `KMP_MCP_DATA_DIR`. Without the override, the
server would register its store in the real known-stores index.
The scratch index lists only the scratch stores. The newest modification in
the real `~/.local/share/kmp/default` is 22:36:23. The audit's first command
ran at 22:37:55, so none of these writes came from the audit.

The real `known-stores.jsonl` changed between two snapshots: 22:29:04 (319
bytes) and 22:54:53 (272 bytes). The 22:54:53 write happened while no run,
test or binary from this audit was active; the previous run ended at
22:53:22. The file contains no scratch path. Another concurrent session is the likely
writer, which is the hazard described in #910.

## 1. Inventory (#901 step 1)

| Layer | Claude Code | Codex |
| --- | --- | --- |
| Plugin on disk | `~/.claude/plugins/cache/underpass/kmp/0.25.0` (0.24.0 also cached); `diff -r` against `plugins/kmp`: identical except `bin/` and `.in_use` | `~/.codex/plugins/cache/underpass/kmp/0.25.0`; identical to `plugins/kmp` |
| Skill file | `skills/kmp-memory/SKILL.md`, 1678 bytes, a router into `kmp_guide` | same file |
| Guide asset on disk | `guide/AGENT.md` key `ingest:guide-sync:1:agent:c7803b87f385e62bc962` | same |
| MCP wiring | `.mcp.json` → `scripts/run-embedded-mcp.sh` (inherits the host environment) | `.codex-plugin/plugin.json` → `kmp-mcp`, `env_vars: [KMP_MCP_DATA_DIR]` only |
| Live connection | This host session: `plugin:kmp:memory` initialize text (on-request routing), 16 tools, store `~/.local/share/kmp/default`. Not called, to avoid writing the real store. Fresh `claude -p` sessions failed: `OAuth session expired and could not be refreshed` (host authentication, not KMP) | `codex exec` 0.155.1, `gpt-6-sol`; server `kmp` connected to the scratch store, proven by `project:atlas-*` refs that exist only there |
| Guide served by `kmp_guide` | n/a (no fresh session) | Scratch store after `guide sync`: `guide_revision_seen` `c7803b87…`, no `GUIDE_UNAVAILABLE` |
| Extra host instructions | — | `~/.codex/AGENTS.md` `<!-- kmp:begin -->` block (2026-08-25), loaded even with `--ignore-user-config` |

`kmp-mcp info` (0.25.0, store format 4) lists the 16 tools: `kmp_ingest`,
`kmp_write_memory`, `kmp_wake`, `kmp_ask`, `kmp_relate`, `kmp_time`,
`kmp_trace`, `kmp_inspect`, `kmp_relabel`, `kmp_condense`,
`kmp_summaries_audit`, `kmp_curate`, `kmp_view_open`,
`kmp_view_apply_intent`, `kmp_view_get_state` and `kmp_guide`. Memory routing
is `on request` (default).

## 2. Skill and guide against behavior (#901 step 2)

| Route | Instruction (ref) | Observed behavior | Status |
| --- | --- | --- | --- |
| Opt-in routing | `SKILL.md` description; `guide:kmp-agent:gate:invocation` / `topics/routing.md` “Invoked, not assumed”; initialize text | Every session that mentioned KMP memory invoked it; initialize carries the same four triggers. Only prompts that mention KMP were tested; the path where KMP is not mentioned was not | Current (positive triggers only) |
| Known-work wake | `topics/routing.md` (“known work enters through `kmp_wake`”), `cards/wake.md` | All 14 sessions woke the about first and followed a returned `continuation` when offered | Current |
| Current/latest via temporal navigation | `topics/routing.md` table; `verbs/time.md` “Temporal intent does not require an explicit date”; `cards/ask.md` last line | Sessions answered after Wake and then used `kmp_ask` with `as_of` for “still true today”, not `kmp_time` (only case 1a rewound). Wake had already returned the state, and routing allows Ask for the remaining gap. The guide does not cause this; see the legacy block below | Current |
| Ask evidence/UNKNOWN | `cards/ask.md` (`answer_status`, `unknown_reason`) | `answered`/`partial`/`unknown` handled; `unknown/anchor_absent_in_selection` on a `validity` read of an entry with no validity interval was reported as unknown, not as false | Current |
| Inspection and completed continuations | `AGENT.md`; `cards/audit.md`; `topics/routing.md` | Relied-on ref inspected in 11/14 sessions. One session added `context_id` to a `continuation`; the native `invalid_argument` said to send it alone, and the retry did | Current |
| Supersedes/conflicts/expiry | `topics/lifecycle.md`; `cards/write.md` | Native proof matches the topic exactly (see table below) | Current; silent on *no marker* (fixed) |
| `kmp_curate`/Jev | `cards/curate.md`, `verbs/curate.md` (opt-in per store, suggestions never written) | See case 4b: Jev suggested `contradicts`, nothing written | Current |

**Stale host instruction outside the released assets.** The user-level
`~/.codex/AGENTS.md` block names `kmp_goto`/`kmp_near`/`kmp_rewind`/`kmp_forward`
(now moves of `kmp_time`). It also says “`kmp_ask` for targeted questions”
first, wakes “on session start”, which bypasses on-request routing, and treats
an empty wake as “no memory yet — start writing one”. Current guidance is
`not_found` versus an empty bounded selection. The block points to a skill
section (“Why the `why` matters”) that now lives in
`guide:kmp-agent:advanced:relations` and mentions a `.kernel/` single-writer
cause from ADR-011. Release #405 removed the installer that wrote these
blocks. Current `setup`, `update` and `uninstall` neither detect nor remove
them. The block was present in every Codex session here and plausibly
explains the Ask-first routing; it does not explain the open-ended wording,
which the no-skill control reproduced.

## 3. Missing guide (#901 step 4)

A store with no guide (`<scratch>/noguide`):

- `kmp_guide {"registration_key":…}` and `kmp_inspect` of
  `guide:kmp-agent:verb:wake` return `isError`, `error.code: not_found` and
  `feedback[0].code: GUIDE_UNAVAILABLE`. The feedback has a `repair` with
  `command: kmp-mcp`, `arguments: [guide, sync, --plugin-root, <plugin-root>]`
  and the store-specific instructions. They say to keep the same binary,
  working directory and `KMP_MCP_DATA_DIR`, stop the holder, sync, restart and
  retry.
- The `kmp-memory` skill routes missing or stale guide assets to `kmp-guide`.
  That skill says to sync only for a confirmed missing guide in the selected
  store, preserving the connection's `KMP_MCP_DATA_DIR`. Same repair.
- **`kmp-mcp doctor` reports every section `ok` on that store.** It does not
  check that the selected store can serve `guide:kmp-agent`. The repair path
  starts from the tool feedback or the skill, not from doctor. This is left
  open as a CLI diagnostic gap; no change here.

A session receiving `GUIDE_UNAVAILABLE` would not be skill-compliant. No
evaluated session received it.

## 4. Fixture (#900)

“Today” is 2026-10-04 UTC. All entries were written with real
`kmp_write_memory` calls by actor `fixture-writer`, `source_kind: human`, label
`component: atlas-export`, `occurred_at = observed_at` as given. Rich links
returned `needs_review` and were resumed through their returned
`next_actions[0]`. Each case has its own about, so cases 3 and 4 are
alternative histories of the same D1.

| About | Entries | Lifecycle |
| --- | --- | --- |
| `project:atlas-open` | D1 decision, observed 2026-08-23T10:00Z: “The nightly ledger export (ATLAS-EXPORT) is produced as CSV because the finance importer only reads CSV.” Evidence: decision-log excerpt | none: no expiry, verification or replacement |
| `project:atlas-replace` | D1 as above; D2 observed 2026-09-20T14:00Z, JSON, with `supersedes` → D1, `why`: “D2 replaces the whole CSV decision: the importer that motivated CSV was replaced by v2, which only accepts JSON.” | D1 superseded |
| `project:atlas-conflict` | D1; DX observed 2026-09-20 (Parquet, data-team note that does not mention D1) with `contradicts` → D1 | live conflict, no supersession |
| `project:atlas-unlinked` | D1 and DX with no relation | for `kmp_curate` |
| `project:atlas-lease` | D1 with `valid_from` 2026-08-23T10:00Z, `valid_until` 2026-09-30T00:00Z | expires |

Native evidence (direct MCP calls, no model):

| Read | Result |
| --- | --- |
| Wake `atlas-open` | `superseded: []`, `conflicts: []`, `expired: []`; D1 evidence `support_clocks.observed_at 2026-08-23`, `ingested_at 2026-10-04`; wake text `Status: ACTIVE` (about status) |
| Rewind `atlas-open` from now | exactly D1 |
| Goto observed 2026-09-01 `atlas-replace` | D1 only; `superseded: []`; `proof.as_of 2026-09-01`, `axis observed` |
| Goto observed now / Wake / Ask `atlas-replace` | `superseded: [{D1, superseded_by D2, why …}]` |
| Ask as_of observed 2026-09-01 `atlas-replace` | `answered`, no superseded marker |
| Wake / Ask `atlas-conflict` | `conflicts: ["DX contradicts D1: …"]`, `superseded: []` |
| Goto validity 2026-09-15 `atlas-lease` | D1 |
| Goto validity now / Wake `atlas-lease` | no entries; `expired: [{D1, valid_until 2026-09-30T00:00:00Z}]` |
| Ask as_of validity now `atlas-lease` | `unknown/no_candidates` |

## 5. Host sessions

Host: `codex exec --json --ephemeral -s read-only`, `gpt-6-sol`, reasoning
`medium`, cwd an empty directory outside the scratch tree. Each session was
fresh and run one after another, never two on one store. The prompt was
“Use KMP memory for about `<about>`. Today is 2026-10-04 (timezone UTC).
<question>”.

“Skill” means the session read `skills/kmp-memory/SKILL.md`. Controls used
`--ignore-user-config`, which drops the plugin, so no skill was listed or read.
The MCP override, initialize instructions and the legacy `AGENTS.md` block
remained. Two controls are not KMP-independent: `control1b-v2` and `control4`
reused the registration key and context of an earlier skill session, so the
cards had already been served. `control1c` ran on a fresh store
(`eval-ctl`, unchanged guide) and is clean. `control1b`, run with only
`plugins."kmp@underpass".enabled=false`, still read `SKILL.md` from disk and
is not a control.

Pre-registered criteria were written before the first case run. For case 1,
**PASS** requires naming D1 as the last *recorded* decision with its observation
date, saying nothing later verifies or replaces it, and neither asserting
that it holds today nor marking it false for its age.

### Unchanged 0.25.0 guide (store `eval`)

| Run | Skill | Calls | Final wording (verbatim, abridged only where marked) | Verdict |
| --- | --- | --- | --- | --- |
| 1a “last recorded decision?” | yes | guide(register) → guide(wake) → wake(as_of, occurred) → guide(ask) → guide(time) → time(rewind, occurred) → guide(audit) → inspect | “The last recorded decision for `project:atlas-open` is **CSV** … Decision D1 was recorded for 2026-08-23 … The KMP history returned no later decision through 2026-10-04.” | PASS |
| 1b “Is D1 still true today?” | yes | guide → guide(wake) → wake → guide(ask) → ask(as_of today, observed → partial) → guide(audit) → inspect | “As of 2026-10-04 UTC, **KMP still records D1 as the current decision** … no superseding decision. That confirms the recorded decision, but KMP has no observation from today confirming the export’s actual output.” | PASS (labels it “current decision”) |
| 1c “Is CSV still the decision?” | yes | guide → guide(wake) → wake → guide(ask) → ask(as_of today, occurred → answered) → inspect | “Yes. As of **2026-10-04 UTC**, KMP still records CSV as the decision … Decision D1, dated August 23 … no recorded supersession.” | **FAIL** (leads with “Yes”; no verification gap) |
| 2 “what did memory know on 2026-09-01?” | yes | guide → guide → guide(wake) → wake(as_of, ingested) → guide(time) → time(goto, observed) | “As of **2026-09-01 UTC**, the recorded decision was … **CSV** … KMP ingested that record retrospectively on **2026-10-04**; the answer above uses when the decision was observed …” | PASS: no D2 leak; ingestion and observation kept apart |
| 3 “current decision, and is D1 still true?” | yes | guide(wake) → wake → wake(continuation) → guide(ask) → ask(as_of, observed → partial) → guide(audit) → inspect(D2) | “… the current KMP decision is **D2: … JSON**. D2 replaced D1 on 2026-09-20 because finance importer v2 … **D1 is no longer the current decision.**” | PASS on supersession and reason; D2's own currency unqualified |
| 4 “current decision?” (conflict) | yes | guide → guide(wake) → wake → wake(continuation) → guide(ask) → ask(show_conflicts → answered) | “KMP records **no settled current format** … D1 … **CSV** … DX … **Parquet** … KMP marks the decisions as contradictory, and neither record says it supersedes the other.” | PASS: did not pick the newer DX |
| 5 “on 2026-09-15, and still true today?” | yes | guide → guide → guide(wake) → wake → guide(ask) → ask(as_of 09-15, validity → answered) → ask(as_of today, validity → unknown) → guide(audit) → inspect | “On **2026-09-15** … **CSV**. … covered 2026-08-23 through 2026-09-30 … **D1 is no longer in effect on 2026-10-04.** KMP records its expiry, but has no later decision establishing today’s export format.” | PASS: expiry, not replacement |
| control1b-v2 (1b) | no | guide → wake → ask(validity → unknown) → ask(observed → answered) → inspect | “… It has no later evidence confirming the export format on 2026-10-04 UTC. **So D1 is the last recorded decision, but whether it is still true today is unknown from KMP memory.**” | PASS |
| control4 (4) | no | guide → wake → wake(continuation + extra args → `invalid_argument`) → wake(continuation) → ask → guide(audit) → inspect ×2 | “… **KMP does not establish one settled format** … conflicting decisions, with no evidence that the later note replaced the earlier one.” | PASS |
| control1c (1c, fresh store) | no | guide → wake(truncated context_id → `invalid_argument`) → … → ask(occurred) → inspect | “Yes. As of **2026-10-04 UTC**, KMP still records CSV as the decision … no superseding decision …” | **FAIL** (same as 1c) |

Case 4b, `kmp_curate` review on `project:atlas-unlinked` with Jev
(`jev-1.13.0`), called natively. It returned two `missing` items
(`proposed_by: jev`, no kernel signal), both `suggested_rel: contradicts`
(0.77 and 0.71, with `supersedes` second at 0.19 and 0.23). Nothing was
written. On `project:atlas-conflict` the declared `contradicts` link was not
suspect. On the single-entry `project:atlas-open` there was nothing to review:
Curate compares stored facts and cannot detect an unrecorded change in the
world, as #900 expected.

### Classification

| Outcome | Cause |
| --- | --- |
| 1c and control1c “Yes … still records CSV” without date or caveat | **Ambiguous guidance (silent).** Same failure with and without the skill. The evidence (`observed_at`, empty lifecycle lists) was present and correctly read; nothing in the cards the route reads said what an unmarked entry means |
| 1b/3 call an unverified entry “current” | same, minor |
| Ask chosen for “still true today” after Wake | Agent choice with a competing legacy host instruction present; guidance correct |
| Continuation with extra args; truncated `context_id` | Agent noncompliance; native error corrected it |
| Unavailable guide | not observed in any evaluated session |
| Missing tool evidence | none |

## 6. Correction and re-run

The canonical sources are the files that `editorial.json` includes through
`text_file`. The `evidence` fields of the touched entries were updated.

- `cards/wake.md`: “An entry without a superseded, conflict or expired marker
  is the last *recorded* state, not a verified present: give its observation
  date, say nothing later verifies or replaces it, and do not mark it false
  for its age.”
- `cards/ask.md`: citations prove what was recorded and when it was observed.
  For “is it still true today”, report the last recorded decision with its
  observation date and whether anything later verifies, replaces or ends it.
  No later record is not confirmation, and age alone is not refutation.
- `topics/lifecycle.md`: new section “No marker is not verification”.
- New lesson `examples/decision-currency.md`
  (`guide:kmp-agent:example:decision-currency`), registered in
  `editorial.json`, `examples:index` and `by-capability.md`. Its replay checks
  are in `scripts/guide_examples/decision_currency_checks.py`; the lifecycle
  prerequisite is in `guide_reads.py`.
- Regenerated with `kmp-release guide assets write --binary
  ~/.local/bin/kmp-mcp` (the same 0.25.0 contract). The new agent asset key
  is `ingest:guide-sync:1:agent:ec378b2abd7a9d2fbe98`. Text changed only in
  `advanced:lifecycle`, `examples:index`, `examples:by-capability`,
  `card:wake` and `card:ask`, plus the new example. Other entries changed only
  their metadata revision, and later entries' teaching clocks moved by one
  minute. `SKILL.md` is unchanged.

Re-run on a fresh store (`eval2`, same fixture, guide synced from this
branch). Every session was served the new card bodies:

| Run | Final wording | Verdict |
| --- | --- | --- |
| v2 1c | “Yes. For `project:atlas-open`, KMP’s latest recorded decision is **CSV** … observed on **2026-08-23** … no later decision replacing or ending it as of **2026-10-04 UTC**; it does not independently verify the export’s current format.” | **PARTIAL**: the observation date and verification gap now appear, but the answer still opens with “Yes”, which the pre-registered criteria count as asserting it holds today |
| v2 1b | “KMP records D1 as the latest decision … dated **2026-08-23 10:00 UTC**. The memory contains no later verification, replacement, or end to D1. So **D1 remains the last recorded decision as of 2026-10-04 UTC**, but KMP does not verify that the export is still being produced as CSV today.” | PASS |
| v2 3 | “… the latest recorded decision is **D2** … **D1 (CSV) is no longer the current decision** … D2 explicitly supersedes it. KMP’s latest observation is the September 20 decision log; it does not independently verify what format an export produced today.” | PASS, now including D2's own currency |

Native replay of the new lesson against the 0.25.0 binary passes all six
checks: the superseded `why`, expiry at the exclusive `valid_until`, no marker
on the open decision, observation ≠ ingestion, no leak before the replacement,
and an Ask citation without a marker. These checks cover native contracts,
not LLM learning; the host runs above cover agent wording.

Checks run: `cargo test -p kmp-release --test guide_editorial_contract --test
plugin_package_contract`, `cargo test -p kmp-mcp --test guide_sync`, `cargo
test -p kmp-adapter-embedded --test guide_bundle`,
`cargo test -p kmp-mcp --test plugin_bootstrap --test plugin_notice --test
stdio_binary`, `scripts/ci/kmp-capability-contract.py`, and
`scripts/guide_examples/replay.py` for `decision-currency`, `first-decision`,
`late-conflict` and `decision-history`. No Rust source changed, so fmt and
clippy were not needed.

## Evidence files

Raw traces are kept locally (gitignored) in
`artifacts/decision-currency-20261004/`. They include every host session's
stream (`runs/*.jsonl`, `*.final.txt`), native reads (`native-results.json`,
`native-trace.jsonl`), the missing-guide probe, the curate review, lesson
replays, the pre-registered `criteria.md`, and the scripts used (`mcp.py`,
`fixture.py`, `native.py`, `curate.py`, `codex-run.sh`, `parse.py`).

The eight authored fixture writes follow, exactly as sent. The `supersedes`
and `contradicts` writes returned `needs_review` and were resumed with their
returned `continuation` unchanged.

```json
{"about": "project:atlas-open", "actor": "fixture-writer", "source_kind": "human", "idempotency_key": "eval900-open-d1", "labels": {"component": ["atlas-export"]}, "occurred_at": "2026-08-23T10:00:00Z", "observed_at": "2026-08-23T10:00:00Z", "memories": [{"id": "d1", "kind": "decision", "summary": "D1: The nightly ledger export (ATLAS-EXPORT) is produced as CSV because the finance importer only reads CSV.", "evidence": "Decision log ATLAS-EXPORT, 2026-08-23 10:00 UTC (platform review, minutes by R. Ilves): \"Decision D1: the nightly ledger export is produced as CSV. Reason: the finance importer only reads CSV.\""}]}
{"about": "project:atlas-replace", "actor": "fixture-writer", "source_kind": "human", "idempotency_key": "eval900-replace-d1", "labels": {"component": ["atlas-export"]}, "occurred_at": "2026-08-23T10:00:00Z", "observed_at": "2026-08-23T10:00:00Z", "memories": [{"id": "d1", "kind": "decision", "summary": "D1: The nightly ledger export (ATLAS-EXPORT) is produced as CSV because the finance importer only reads CSV.", "evidence": "Decision log ATLAS-EXPORT, 2026-08-23 10:00 UTC (platform review, minutes by R. Ilves): \"Decision D1: the nightly ledger export is produced as CSV. Reason: the finance importer only reads CSV.\""}]}
{"about": "project:atlas-replace", "actor": "fixture-writer", "source_kind": "human", "idempotency_key": "eval900-replace-d2", "labels": {"component": ["atlas-export"]}, "occurred_at": "2026-09-20T14:00:00Z", "observed_at": "2026-09-20T14:00:00Z", "read_context": {"inspected_refs": ["project:atlas-replace:entry:decision:d1-the-nightly-ledger-export-atlas-export-is-produced-as-csv-be-42c06890a3690a52"]}, "memories": [{"id": "d2", "kind": "decision", "summary": "D2: The nightly ledger export (ATLAS-EXPORT) moves to JSON, replacing D1 (CSV), because finance importer v2 reads JSON and no longer accepts CSV.", "evidence": "Decision log ATLAS-EXPORT, 2026-09-20 14:00 UTC (platform review, minutes by R. Ilves): \"Decision D2: the nightly ledger export moves to JSON and replaces D1 (CSV). Reason: the finance importer v2, deployed 2026-09-18, reads JSON and no longer accepts CSV.\"", "connect_to": [{"ref": "project:atlas-replace:entry:decision:d1-the-nightly-ledger-export-atlas-export-is-produced-as-csv-be-42c06890a3690a52", "rel": "supersedes", "class": "evidential", "confidence": "high", "why": "D2 replaces the whole CSV decision: the importer that motivated CSV was replaced by v2, which only accepts JSON.", "evidence": "Decision log ATLAS-EXPORT, 2026-09-20 14:00 UTC (platform review, minutes by R. Ilves): \"Decision D2: the nightly ledger export moves to JSON and replaces D1 (CSV). Reason: the finance importer v2, deployed 2026-09-18, reads JSON and no longer accepts CSV.\""}]}]}
{"about": "project:atlas-conflict", "actor": "fixture-writer", "source_kind": "human", "idempotency_key": "eval900-conflict-d1", "labels": {"component": ["atlas-export"]}, "occurred_at": "2026-08-23T10:00:00Z", "observed_at": "2026-08-23T10:00:00Z", "memories": [{"id": "d1", "kind": "decision", "summary": "D1: The nightly ledger export (ATLAS-EXPORT) is produced as CSV because the finance importer only reads CSV.", "evidence": "Decision log ATLAS-EXPORT, 2026-08-23 10:00 UTC (platform review, minutes by R. Ilves): \"Decision D1: the nightly ledger export is produced as CSV. Reason: the finance importer only reads CSV.\""}]}
{"about": "project:atlas-conflict", "actor": "fixture-writer", "source_kind": "human", "idempotency_key": "eval900-conflict-dx", "labels": {"component": ["atlas-export"]}, "occurred_at": "2026-09-20T14:00:00Z", "observed_at": "2026-09-20T14:00:00Z", "read_context": {"inspected_refs": ["project:atlas-conflict:entry:decision:d1-the-nightly-ledger-export-atlas-export-is-produced-as-csv-be-a30023abcc93efe0"]}, "memories": [{"id": "dx", "kind": "decision", "summary": "DX: The nightly ledger export (ATLAS-EXPORT) is produced as Parquet so the warehouse loader can read it directly.", "evidence": "Data team note ATLAS-EXPORT, 2026-09-20 14:00 UTC (author: K. Osei): \"Decision DX: the nightly ledger export is produced as Parquet so the warehouse loader can read it directly.\" The note does not mention D1 or the finance importer.", "connect_to": [{"ref": "project:atlas-conflict:entry:decision:d1-the-nightly-ledger-export-atlas-export-is-produced-as-csv-be-a30023abcc93efe0", "rel": "contradicts", "confidence": "high", "why": "DX and D1 name different single formats (Parquet vs CSV) for the same nightly export; neither source says one replaces the other.", "evidence": "Data team note ATLAS-EXPORT, 2026-09-20 14:00 UTC (author: K. Osei): \"Decision DX: the nightly ledger export is produced as Parquet so the warehouse loader can read it directly.\" The note does not mention D1 or the finance importer."}]}]}
{"about": "project:atlas-unlinked", "actor": "fixture-writer", "source_kind": "human", "idempotency_key": "eval900-unlinked-d1", "labels": {"component": ["atlas-export"]}, "occurred_at": "2026-08-23T10:00:00Z", "observed_at": "2026-08-23T10:00:00Z", "memories": [{"id": "d1", "kind": "decision", "summary": "D1: The nightly ledger export (ATLAS-EXPORT) is produced as CSV because the finance importer only reads CSV.", "evidence": "Decision log ATLAS-EXPORT, 2026-08-23 10:00 UTC (platform review, minutes by R. Ilves): \"Decision D1: the nightly ledger export is produced as CSV. Reason: the finance importer only reads CSV.\""}]}
{"about": "project:atlas-unlinked", "actor": "fixture-writer", "source_kind": "human", "idempotency_key": "eval900-unlinked-dx", "labels": {"component": ["atlas-export"]}, "occurred_at": "2026-09-20T14:00:00Z", "observed_at": "2026-09-20T14:00:00Z", "memories": [{"id": "dx", "kind": "decision", "summary": "DX: The nightly ledger export (ATLAS-EXPORT) is produced as Parquet so the warehouse loader can read it directly.", "evidence": "Data team note ATLAS-EXPORT, 2026-09-20 14:00 UTC (author: K. Osei): \"Decision DX: the nightly ledger export is produced as Parquet so the warehouse loader can read it directly.\" The note does not mention D1 or the finance importer."}]}
{"about": "project:atlas-lease", "actor": "fixture-writer", "source_kind": "human", "idempotency_key": "eval900-lease-d1", "labels": {"component": ["atlas-export"]}, "occurred_at": "2026-08-23T10:00:00Z", "observed_at": "2026-08-23T10:00:00Z", "memories": [{"id": "d1", "kind": "decision", "summary": "D1: The nightly ledger export (ATLAS-EXPORT) is produced as CSV from 2026-08-23 until 2026-09-30, when the finance importer contract ends.", "evidence": "Decision log ATLAS-EXPORT, 2026-08-23 10:00 UTC (platform review, minutes by R. Ilves): \"Decision D1: the nightly ledger export is produced as CSV from 2026-08-23 until 2026-09-30 00:00 UTC, when the finance importer contract ends. A new decision is due before then.\"", "valid_from": "2026-08-23T10:00:00Z", "valid_until": "2026-09-30T00:00:00Z"}]}
```

## Left open

- `kmp-mcp doctor` does not report a selected store that cannot serve
  `guide:kmp-agent`; the repair is reachable only through `GUIDE_UNAVAILABLE`
  feedback or the skill.
- A legacy `<!-- kmp:begin -->` block in `~/.codex/AGENTS.md` names removed
  tools and is loaded in every Codex session. Setup, update and uninstall do
  not detect or remove these blocks. Removing it on this machine is the
  user's decision.
- Codex forwards only `KMP_MCP_DATA_DIR` to the plugin server, so an isolated
  evaluation still registers its store in the real known-stores index unless
  the server environment is overridden (#910).
- Ask reports function words as `proof.missing` (`that`, `true`, `Has`) and
  returns `partial` for yes/no currency questions. This did not change any
  answer here; it is an independent retrieval observation for #900.
- Claude Code host sessions were not run: `claude -p` authentication had
  expired. The skill and guide files are identical in both hosts, so the
  result is expected to transfer, but that is not demonstrated here.
- With n = 1 per prompt, phrasing still matters: “Is X still the decision?”
  keeps a leading “Yes” even though the rest of the answer is now qualified.
