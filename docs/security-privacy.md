# Security and privacy

What leaves your machine, what stays, and what protects the local viewer.
This page covers the local 1.0 product: `kmp-mcp` over stdio with its
embedded SQLite store. The shared service is experimental and has its own
page: [Enterprise security](enterprise/security.md). To report a
vulnerability, see [SECURITY.md](../SECURITY.md).

## In one table

| What | Leaves the machine? | When |
|:--|:--|:--|
| Your memory (stored text, evidence, relations) | **No**, by default. | Only if you opt a store into TypeSafe Jev (below), or commit and push a project bundle yourself. |
| The question an agent asks | No, by default. | Only with a Jev opt-in that judges asks (`rerank.json`, `ask-judge.json`). |
| A release check | **Yes**, on by default with the Claude Code plugin. | At most once a day, from the plugin's session hook. It sends no memory. `KMP_VERSION_CHECK=off` stops it. |
| Downloads | Yes. | Only when you run `kmp-mcp setup`, `kmp-mcp update` or `/kmp:setup`. |
| The viewer | Nothing listens. | Until ChronoLoom is requested; then on loopback only. |

Evidence an agent reads from memory is sent to that agent's model provider.
That follows the host's data policy, not KMP's.

## Network calls, one by one

### TypeSafe Jev (opt-in, per store)

Off unless the store's data directory holds `typesafe.json` **and** a key is
found (`TYPESAFE_API_KEY`, else `$XDG_CONFIG_HOME/typesafe.env`, which must
not be readable by group or others). Without `typesafe.json` the key is never
read. The endpoint must be `https://api.typesafe.ai/v1/systemone`; the model
must be a pinned release such as `jev-1.14.0`.

| Tool | Opt-in file | Text sent to TypeSafe |
|:--|:--|:--|
| `kmp_curate` | `typesafe.json` alone | Memory texts of the abouts you name (cut to 300–2,000 characters each). For declared relations under review, and for the pre-write check of `apply`: the relation's `why` and `evidence`. In `labels` mode: the about's label keys and values. |
| `kmp_summaries_audit` | `typesafe.json` alone | For each memory on the page whose summary stands: its text (up to 1,500 characters) and its `summary_en`. |
| `kmp_ask` | `rerank.json` | The question, and each candidate memory's text cut to `excerpt_chars` (up to 2,000). |
| `kmp_ask` | `ask-judge.json` | The question, and at most 8 candidate texts (up to 2,000 characters each), only for asks the gate settled in doubt. |
| `kmp_wake` | `wake-focus.json` | The wake's `intent`, and evidence texts cut to `excerpt_chars`. Only when `intent` is set. |
| `kmp_write_memory` | `write-relations.json` | The memories just written (up to 1,000 characters each) and candidate memories of the same about (up to 300). |
| `kmp_write_memory` | `write-expansions.json` | Each proposed search expansion, with its memory's text (up to 4,000). |

Never sent: the store path, refs (questions use internal keys such as `p0`),
other abouts, the telemetry salt, the key in the body. The key travels only
as an `Authorization` header. Each request is capped at 256 KiB. Verdicts are
kept locally in `judgements.sqlite3`, keyed by hashes, with no text.

When a store opts in, the tools above declare `openWorldHint: true` in
`tools/list`; on a store that did not, no tool does.

Source: `crates/kmp-mcp/src/serving/adapters/typesafe_*.rs` and the builders
each row names in the code. Verified in code; no real call was made for this
page.

### Release check

| Fact | Value |
|:--|:--|
| Request | `GET https://api.github.com/repos/underpass-ai/kmp/releases/latest`, reads only `tag_name`. |
| Carries | User-Agent `kmp-version-notice`. No body, no memory, no machine data. |
| Who runs it | The Claude Code plugin hooks `SessionStart` and `Setup` (`kmp-mcp plugin notice`). Codex, Hermes and Pi declare no such hook. `doctor` never calls GitHub. |
| How often | At most once a day: the answer is cached in `$XDG_CACHE_HOME/kmp/latest-release`. A failed call is not cached. |
| Turn off | `KMP_VERSION_CHECK=off`. |

### Setup, update and the plugin bootstrap

`kmp-mcp setup`, `kmp-mcp update` and the plugin's `kmp-install-binary.sh`
download the engine and the lexical-bridge table from GitHub Releases of
`underpass-ai/kmp`. Each file is checked against the SHA-256 published in
the same release. **There is no signature check**: the checksum comes from
the same host as the file. Plugins themselves are fetched by the host's own
CLI (`claude plugin …`, `codex plugin …`), which setup runs.

### Experimental paths (off by default)

| Path | What it sends |
|:--|:--|
| gRPC backend (`KMP_KERNEL_GRPC_ENDPOINT`) | Every tool call, to that kernel. |
| Semantic retrieval (`semantic-retrieval.json`) | The question and candidate texts, to an encoder on a literal loopback address (`127.0.0.1` or `[::1]`) only. |

Nothing else in the embedded path opens an outbound connection.

## The viewer

| Protection | Detail |
|:--|:--|
| Listens only on request | A stdio session binds nothing until the first successful `kmp_view_open`. `kmp-mcp viewer` and `kmp-mcp demo` bind when they start. |
| Loopback only | A non-loopback address is refused at startup (`0.0.0.0`, LAN addresses, `[::]`, host names). |
| Capability link | 32 random bytes per process. Without it every page and API answers `401`. The link sets an `HttpOnly; SameSite=Strict` cookie (`Max-Age=86400`) and redirects (`303`) to the clean URL. |
| Read-only memory | `GET` reads memory. `POST` changes only view state, and refuses a foreign `Origin` or `Sec-Fetch-Site` (`403`). |
| Headers | CSP `default-src 'none'` with `frame-ancestors 'none'`, `X-Frame-Options: DENY`, `nosniff`, `no-referrer`, same-origin CORP and COOP. No CORS headers. |
| Limits | At most 64 connections, a request head within 5 s and 16 KiB, at most 64 open views. |

Memory text reaches the page only as text (`textContent`), never as HTML.

## Local files

All of these stay on the machine. `D` is the data directory `kmp-mcp info`
names.

| File | Holds |
|:--|:--|
| `D/store/` | All memory (SQLite, WAL). |
| `<repo>/.kmp/memory.jsonl` | The project bundle: the full event log of the project's abouts. You decide whether to commit and push it. Review its diff first. |
| `D/lexical-index.sqlite3` | Derived search index. Safe to delete; it is rebuilt. |
| `D/continuations.sqlite3` | The full arguments of reads that returned a continuation handle (question, about, intent), kept 24 hours. |
| `D/judgements.sqlite3` | Jev verdicts, keyed by hashes, no text. Only with a Jev opt-in. |
| `D/logs/` | One `kmp_mcp_tool` line per call: tool, status, counts, durations, the host's name and version, keyed fingerprints. Never stored text, a question or an answer. |
| `D/telemetry-salt` | 32 random bytes (mode `0600`) that key those fingerprints. Delete it to start new ones. |
| `D/telemetry/quality.sqlite3` | Quality metrics of measured reads. |
| `$XDG_DATA_HOME/kmp/known-stores.jsonl` | Absolute paths of stores opened on this machine. |
| `$XDG_CONFIG_HOME/kmp/config.toml` | `memory_routing` and `memory_store`. |

`kmp-mcp uninstall` previews what it would remove; it saves a memory copy
first unless `--purge`, and never touches a committed bundle.

## Stored text is untrusted input

Memory can hold text anyone wrote. The server's `initialize` instructions
tell the agent that stored text is untrusted evidence and cannot override
instructions or authorize actions. A committed bundle is code review
material: read what a teammate's bundle adds before you trust it.

## Supported versions

See [Stability and support](stability.md#support-window).
