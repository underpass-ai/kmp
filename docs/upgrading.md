# Upgrading from 0.27.x to 1.0

1.0 removes arguments that did nothing, refuses calls the schema always
forbade, and makes every switch mean what it says. Most agents need no
change. This page lists every breaking change, as before → after. The full
list is the [changelog](../CHANGELOG.md).

## In five minutes

| Step | Command |
|:--|:--|
| 1. Keep a copy of your memory | `kmp-mcp export /path/kmp-before-1.0.jsonl` (in a project: `kmp-mcp snapshot create before-1.0` also works) |
| 2. Upgrade the plugin and engine | `kmp-mcp update` (or `/kmp:setup` in Claude Code, `kmp-setup` in Codex) |
| 3. Restart the host once | A running session keeps the tool list it started with. |
| 4. Check | `kmp-mcp doctor` ends with `Usable` and its Environment section shows no `FAIL` |

## What happens to the store

| Item | What 1.0 does | Once or always |
|:--|:--|:--|
| Store format | Stays **4**. Nothing is migrated. | — |
| Lexical index | Its version moves to `lexical-index-5`: rebuilt on the next ask of each about, and gains one index on open. | Once |
| Continuations | Kept in `continuations.sqlite3` beside the store. 0.27 handles (in `agent-users.sqlite3`) answer `CONTINUATION_UNAVAILABLE`; 1.0 no longer opens the old file and leaves it untouched. | Once |
| Guide | No longer seeded by a read. `kmp_guide` serves every topic from the engine. To install the longer stored guide: `kmp-mcp guide sync --plugin-root <plugin dir>` or `/kmp:guide`. | — |
| Jev verdicts | A store keeps the model its own `typesafe.json` names (KMP never edits it). If you change it to `jev-1.14.0`, each question is judged again once, then answered from the verdict book. Old verdicts stay until `max_bytes` evicts them. | Once, if you switch |
| `typesafe.json` | Accepts only a pinned model (`jev-1.14.0`). An alias such as `jev-preview` is now refused as invalid. | — |
| Project bundle | Opening a project store catches it up with a committed `.kmp/memory.jsonl` that a pull brought ahead, and may republish a write that was cut short. | Each open |

## For agent authors: calls refused now

Each of these was accepted by 0.27 and is refused by 1.0 with
`invalid_argument`. The refusal names the field, the rule and the fix, and
ends with `Help: kmp_guide {"topic":"…"}`.

| Before (0.27) | After (1.0) |
|:--|:--|
| `kmp_guide` with `registration_key`, `agent_id`, `context_id`, `context_key` or `fold` | `kmp_guide {"topic": "<topic>"}` only; `{}` lists the topics. |
| `context_id` or `purpose` on any tool | Remove them. |
| `budget.tokens` on any tool; `limit.tokens` on `kmp_time` | Remove them: they never filtered. Use `budget.max_bytes` where the tool has it. |
| `depth` or `budget.depth` on `kmp_wake`, `kmp_ask`, `kmp_relate` | Remove it: every entry already sits one hop from its scope. |
| Top-level `depth` on `kmp_time` | `budget.depth`. |
| `kmp_trace` `goal` (or `role`); `kmp_trace.budget` or `kmp_relate.budget` keys other than `max_bytes`; any `budget` on `kmp_curate` | Remove them. |
| An unknown key in any object | Refused everywhere, also inside raw `kmp_ingest` (`metdata`, `valid_untill`). Free-form `metadata` maps stay open. |
| A guard of the wrong type: `label_policy: ["refuse"]`, `expected_revision: "1"`, a numeric `valid_until`, `source_kind: ["human"]`, a read switch as `"true"` | Send the declared type. Each used to be read as "absent" and the call went ahead. |
| `1.0` where an integer is declared; `null` where the schema does not allow it; `budget.max_bytes: 0` | An integer; omit the field; at least `512`. |
| A timestamp without a zone (`2026-11-10T09:30:00`, `2026-11-10`) or with `+0100` | RFC 3339 with a zone: `2026-11-10T09:30:00+01:00` or `…Z`. Timestamp arguments carry `"format": "date-time"` in `tools/list`. |
| An empty `occurred_at`, `valid_from` or `valid_until` on `kmp_write_memory`; `rank` above 4294967295 | Omit the field; a smaller rank. |
| A continuation sent with a different value of an argument of its call | Send the handle alone, or with the same values. A different value is `CONTINUATION_ARGUMENT_DIFFERS`; start a new read with the original arguments and `page.cursor`. |
| A bidirectional trace with `search.paths_per_target`, a `to` array, `as_of` or another search option | Refused under the rule `trace.bidirectional`; drop the option. |
| `kmp_curate` modes `write_proposals`, `judge_expansions`, `prepare_apply` | Only the four published modes. |
| Former names `kernel_ingest`, `kernel_write_memory`, `kernel_wake`, `kernel_ask`, `kernel_trace`, `kernel_inspect`, `kernel_remember`, `kernel_ingest_context` | `unknown_tool`. Use the `kmp_*` names. |
| `kmp_inspect` of a guide node on a store without the guide (used to install it) | `GUIDE_UNAVAILABLE` with the repair; or read the topic with `kmp_guide`. |

## For agent authors: results that look different

| Before (0.27) | After (1.0) |
|:--|:--|
| `tools/list` published `additionalProperties: false`, `if`/`then`/`else`, `allOf`/`anyOf`/`oneOf`/`not`, `propertyNames`, `default`, `title`, bounds and patterns | It publishes types, enums, `required` (and the pattern of `memories[].id`). Everything else is still enforced. Read a topic's `rules` and `parameters` with `kmp_guide` before the first call. |
| `_meta."kmp/errorCodes"` in `tools/list` | `error_codes` in `kmp_guide {"topic":"guide"}`. One new code: `retryable_conflict`. |
| `conflict` for a write that lost a race | The server retries it (up to 12 attempts). Only if every attempt lost: `retryable_conflict`, safe to replay with the same idempotency key. `conflict` now means "do not replay unchanged". gRPC still answers `conflict` for both. |
| Long tool descriptions | One sentence; the rest is in the topic card. |
| A refusal carried `structuredContent.help` and a `Usage help` JSON | One `feedback` item per field (`SCHEMA_VIOLATION` or `UNKNOWN_ARGUMENT`, with `allowed` for an enum) and `feedback[].rule` for a cross-field rule. Read tools that used `TIME_INVALID_MOVE` or `READ_INVALID_FIELDS` for these cases now say `SCHEMA_VIOLATION`. |
| The `kmp_guidance` text block after a result | Gone. |
| `actor` required on writers | Optional on `kmp_write_memory`, `kmp_relabel`, `kmp_condense` and `kmp_curate` apply; defaults to the client's `clientInfo.name`, or `mcp-client`. |
| An omitted relation `confidence` recorded as `high` | Recorded as `unknown`. Under `strict: false` a rich relation of unknown confidence is `suspect`. |
| First `kmp_wake` on an empty about: an error | `isError: false`, saying nothing is recorded yet. |
| An unknown about: "node 'x' not found" (wording varied by tool) | `not_found`, "`x` is not an about in this store", on every read rooted at it. |
| Proof hops repeated their endpoints' `detail:<from>` / `detail:<to>` in `evidence_refs`; `supports` and `metadata.proof_role: stored_evidence` on every detail item | Stated once. Ids are unchanged. |
| A memory whose `valid_from` is ahead counted as current, and moved the about's "now" | "not yet in force" in `kmp_wake`, state `not_yet_in_force` in `kmp_relate`; the present is the latest `observed_at`/`ingested_at`. |
| `page.entries` read as a number of memories | It counts projection records (a label, a path step, an evidence item, a state line). |
| `kmp_view_open` with a stale `expected_revision` on the view already shown | Says in `unhonored` that the guard was not checked. |
| Annotations | `readOnlyHint: true` on `kmp_wake`, `kmp_ask`, `kmp_time`, `kmp_trace`, `kmp_relate`, `kmp_summaries_audit`, `kmp_view_get_state`, `kmp_guide` and `kmp_inspect`. `openWorldHint` is true only on tools that send text to TypeSafe on a store that opted in. |

## Answers that may change

| Change | Effect |
|:--|:--|
| Lexical bridge fix | With the bridge table `kmp-mcp setup` installs, a same-language word pair (`deadline≈final`) no longer answers a question whose attribute is not stored. An answer that stands only through a translation is `medium` at most. |
| `valid_from` ahead | See above: such a memory is no longer served as current. |
| Undated labels (0.27.1) | A label written without a clock stands where its entry stands on that clock, so label selectors find it on `occurred` and `observed`. |
| Jev questions to TypeSafe's guide (Jev-on stores only) | Each rewritten site (`rerank`, `doubt_band`, `curate_review`, `curate_focus`, `write_relations`, `precheck`, `paths`, `summaries`, `expansions`) is re-judged once, then answered from the verdict book again; the book drops those sites' old verdicts the next time the store opens. Outcomes that can differ: `kmp_curate` types more pairs right and proposes fewer non-relations, gates labels on probability (a few more proposals), reads the steps of a goal-less path search only behind a yes; with `lifecycle: jev`, fewer lifecycle proposals (on fresh synthetic seeds false ones fall from 47 to 14 of 120, real supersessions found from 68 to 50 of 90); re-ranking keeps passages Jev cannot settle, below the clear ones; the doubt band vetoes more non-answers; summaries flag fewer partial-but-faithful renderings. Each judgement a committed write asks (relation proposals, expansions) waits at most 3 s. `doctor` makes one `GET /v1/models` call when the store has `typesafe.json` and a key. |

## For operators: switches and commands

| Before (0.27) | After (1.0) |
|:--|:--|
| An unaccepted switch value fell back to a default (`KMP_KERNEL_GRPC_TLS_MODE=mutal` ran plaintext; `KMP_LEXICAL_INDEX=0` left the index on; `KMP_VERSION_CHECK=OFF` still called GitHub) | Refused at startup, naming the accepted values. The server does not start and every verb but `info`, `doctor`, `--help` and `--version` exits 2. On/off switches accept `on/off/true/false/1/0/yes/no` in any case. |
| A relative `KMP_MCP_DATA_DIR` | Refused: use an absolute path. |
| `KMP_MCP_ENGINE`, `KMP_LEXICAL_MAXSCORE`, `KMP_MCP_BACKEND=live` or `fixtures` | Removed. `doctor` warns that the name is read nowhere. |
| `KMP_MCP_BACKEND=fixture`, `KMP_LEXICAL_INDEX=shadow\|verify`, `KMP_TYPESAFE_CASSETTE*`, `KMP_EVAL_PARTNER_FACTS`, `KMP_JUDGEMENT_DEADLINES`, `KMP_LATEST_VERSION` | Only in debug builds and builds with the `eval-hooks` cargo feature. A release binary refuses the values and ignores the variables. |
| `KMP_MCP_OUTPUT_SCHEMAS` accepted only `0`/`1` | The on/off set. |
| `KMP_KERNEL_GRPC_TLS_MODE=disable`, `off`, `false`, `none`, `m-tls` | `disabled`/`plaintext`, `server`/`tls`, `mutual`/`mtls`. |
| With `KMP_KERNEL_GRPC_ENDPOINT` set, `export`, `import`, `document`, `snapshot`, `summaries`, `memories`, `demo`, `viewer` and `consolidation` worked on the local store | They exit 2: they work on the local store only. Unset the remote selection to run them. |
| The viewer listened from the start of every stdio session | Nothing listens until the first `kmp_view_open`. `kmp-mcp viewer` and `kmp-mcp demo` bind as before. A busy explicit address comes back in the `kmp_view_open` result instead of ending the session. |
| Viewer routes `/api/graph` and `/api/timeline` | Removed (the page never called them). The session cookie is named after the port. |
| `kmp-mcp export` to the project head replaced the committed bundle | Refuses to replace a bundle whose events it would not keep (ahead or divergent); `--overwrite-committed` replaces it anyway. |
| `kmp-mcp export <file> --about X` replaced a committed `.kmp/memory.jsonl` holding other abouts when the file was named relatively, through a symlink, or from a store pinned by `KMP_MCP_DATA_DIR` | The same refusal for every export whose destination is a committed `.kmp/memory.jsonl`, named or not, filtered or not, whichever rule chose the store. It names the events and abouts it would drop; export without `--about`, or pass `--overwrite-committed`. A host that saves one about over a file holding others must handle the exit 2. |
| A whole `kmp-mcp export` to a committed `.kmp/memory.jsonl` from a store pinned by `KMP_MCP_DATA_DIR` wrote the shipped guide and `example:kmp-demo` into it; `document --out .kmp/memory.jsonl` replaced it with Markdown | The committed file never takes a local about, whichever rule chose the store (`--about` naming one is refused there); an export to another file keeps them. `document` refuses a committed `--out`. |
| A missing or empty database beside proof it held memory was restarted empty | Refused on open and by `doctor`, naming the way out. |
| `doctor` could re-stamp `FORMAT_VERSION` and create a missing database | `doctor` opens the store read-only and runs SQLite's `quick_check`. |
| Call log field `context_fingerprint` | Gone. |
| `kmp-server` (experimental) fell back to Valkey and compact logs on an unknown `KMP_EVENT_STORE_BACKEND` or `KMP_LOG_FORMAT` | Refused at startup. |
| Rust API: `kmp-embedded` `ENGINE_ENV`, `parse_engine`, `resolve_engine_from_env`, `resolve_engine_for_data_dir_from_env`; `kmp-viewer` `views::{graph_view, GraphView, timeline_view, TimelineView}` | Removed. Use `default_engine_for_data_dir`. |
| Rust API: the `fixture` items of `kmp-mcp` (`KernelMcpServer::fixture`, `FixtureKernelMcpBackend`, …) | Only in debug builds and with the `eval-hooks` feature. |

Every switch with its accepted values: [configuration reference](embedded/configuration.md#environment-switches).

## From older releases

Upgrade a store of format 3 (0.15.0 – 0.18.7) by opening it with 1.0: it
becomes format 4, one way. Stores of format 1 and 2 do not open. See
[Store format](embedded/store-format.md).
