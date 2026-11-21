# Stability and support

KMP follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html). This
page says what a 1.x release promises, what it does not, and how long 1.x is
supported.

## What 1.0 promises

The promise covers **local use**: one `kmp-mcp` process beside your agent,
with an embedded SQLite store on your machine.

| Surface | Promised in 1.x |
|:--|:--|
| MCP tools over stdio | Tool names; argument names, types, enums and `required` as `tools/list` publishes them; the cross-field rules by id (`write.labels`, `time.cursor`, …); the result fields agents act on (`answer_status`, `unknown_reason`, `proof`, `projection`, `page`, continuation handles); the closed set of error codes in `kmp_guide {"topic":"guide"}` (`invalid_argument`, `not_found`, `conflict`, `retryable_conflict`, `unavailable`, `backend_error`, `unknown_tool`); the `kmp_guide` topics. |
| The guide | `kmp_guide {topic}` is a stateless lookup served from the binary. It reads no store and writes none. |
| Reads | A read never writes memory. |
| The schema | What the published schema and the documented rules forbid is refused, with the rule and its fix. A refused call never takes effect. |
| CLI | The verbs, their flags and exit codes as `kmp-mcp <verb> --help` prints them ([CLI reference](embedded/cli.md)). |
| Store | Store format 4 (SQLite). Every 1.x release opens a store written by 1.0. See [Store format](embedded/store-format.md). |
| Export and import | Bundle format 3 with event formats 2 and 3. A bundle written by 1.0 imports into every 1.x. |
| Configuration | The environment switches, the optional files beside the store and the `config.toml` keys in the [configuration reference](embedded/configuration.md). |
| The viewer | ChronoLoom, opened by `kmp_view_open`, `kmp-mcp viewer` or `kmp-mcp demo`, on loopback behind a capability link. The view tools are MCP tools and follow the first row. |

## What may change in a 1.x minor release

| Change | Allowed in a minor? |
|:--|:--|
| A new tool, a new optional argument, a new enum value in a result, a new result field | Yes. |
| A new CLI verb or flag, a new optional file, a new environment switch | Yes. |
| Better ranking, so a question cites a different memory or answers where it said `UNKNOWN` | Yes. Ranking changes are measured against the judged baselines and the memory bench; a workspace test asks the lexical-bridge stress negatives on every CI run. |
| Wording of messages, summaries, card text and `--help` prose | Yes. Act on codes, rule ids and fields, not on text. |
| Faster reads, smaller results within the same fields | Yes. |
| A derived file beside the store rebuilt once (the lexical index) | Yes, with a changelog line. |
| A bug fix that refuses a call the docs already forbade | Yes. The rule was the promise; the old acceptance was the bug. |
| Removing or renaming a promised tool, argument, field, verb, flag, switch or file | **No.** Only in 2.0. |
| A store or bundle that a 1.x release can no longer open | **No.** |

A behaviour change that is not a fix is called out under **Changed** in the
[changelog](../CHANGELOG.md), with what to do.

## Experimental: outside the promise

These ship in the repository and may change or go away in any release:

| Surface | Why |
|:--|:--|
| MCP over HTTP (`kmp-mcp-http`) | Shared-service path; needs an operator. |
| The gRPC API `underpass.rehydration.kernel.v1beta1` and the gRPC backend of `kmp-mcp` (`KMP_KERNEL_GRPC_ENDPOINT`) | Versioned `v1beta1`; `kmp_curate` and `kmp_summaries_audit` do not answer there. |
| `kmp-server` and the non-embedded adapters (Valkey, NATS, Neo4j), the container image and the Helm chart | The [enterprise topology](enterprise/README.md). |
| Semantic retrieval (`semantic-retrieval.json`) | A local encoder you operate. |

The Rust API of the published crates is not covered by this page: each
crate's README says how stable its API is.

## Optional and opt-in

TypeSafe Jev is optional. It is off unless a store opts in with
`typesafe.json` and a key, and it is pinned to `jev-1.14.0`. A store keeps
the model its own `typesafe.json` names; KMP never rewrites that file. What
each opt-in sends is in [Security and privacy](security-privacy.md).

## Known limits of 1.0

| Limit | Detail |
|:--|:--|
| Viewer size | ChronoLoom is promised to stay fluid up to **10,000 memories per about**. Above that it works but slows: each change of the time window reads the whole about. Windowed reading is planned for 1.1. |
| Read latency on a large about | Measured on one about with the lexical bridge table `kmp setup` installs, release build, p50 of 7 calls (PR #949, 21 Nov 2026, code at 9b6fc2da, `docs/development/performance-baseline.md`): at 10,000 memories an ask takes about 0.46 s (about 0.5 s at the median of the bench's questions; about 0.33 s without the table), a first wake about 0.65 s, a repeated wake about 0.1 s and a rewind about 0.14 s; at 100,000, an ask about 5 s (about 3 s without the table), a wake about 7.5 s and a rewind about 4 s, with a peak of about 4.4 GB. A sidecar written by an earlier version is rebuilt on the first ask after the upgrade (about 4 s at 10,000 memories, 50 s at 100,000). |
| False answers | Measured, not proved. On the memory bench, Jev off, the false-answer rate is 0.000 with and without the shipped lexical-bridge table, on a real store (124 questions, 64 of them about something not stored) and on a synthetic store of 1,000 memories (integration branch at `c517577b`, 21 Nov 2026). As reported with PR #942 (not re-measured here): on 360 synthetic questions about attributes that are not stored, the shipped table answers none; a wrong translation between Spanish and English can still answer, at `medium` at most (6 of 264 cross-language stress cases); one stray Spanish function word can make an English question read as Spanish (12 of 360, all `medium`). Only Spanish and English are read as languages. |
| Languages | Questions in Chinese, Japanese or Thai are not segmented into words. See [Languages](embedded/languages.md). |
| Windows | Release builds exist for Windows x86_64, but `kmp-mcp setup` and `update` do not install an engine there. See [Install](install.md#platforms). |

## Deprecation

How something promised is retired:

1. It is listed under **Deprecated** in the changelog of the release that
   decides it, with its replacement.
2. It keeps working, unchanged, for the rest of 1.x.
3. Where it can, `kmp-mcp doctor` warns while it is still in use.
4. It is removed only in the next major release.

## Support window

The 1.x series is supported for **one year from the release of 1.0.0**.
During that year security fixes and fixes for wrong answers ship in a new
1.x release. Fixes are not backported to earlier 1.x releases: upgrade within
1.x to get them, which this policy keeps safe. Experimental surfaces get
fixes on a best-effort basis only.

To report a vulnerability, see [SECURITY.md](../SECURITY.md).
