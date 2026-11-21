# Store configuration and retrieval defaults

Every setting `kmp-mcp` reads: environment switches, the optional files
beside the store, and the user config file. Check `kmp-mcp --version` before
comparing an installed engine with this page.

## Select the memory first

Run this from the host's working directory with the same environment:

```bash
kmp-mcp doctor
```

It names the data directory, the rule that chose it, the switches in effect
and the verdict on every optional file. `kmp-mcp config` shows the saved
selection alone.

`KMP_MCP_DATA_DIR` wins over everything; then the nearest project root's
`.kernel/` (a Git checkout, whose worktrees share its store); then the saved
`config memory-store` selection; then the per-user default.
Optional JSON files go in that selected data directory, beside `store/`,
not inside it. Restart the MCP process after changing startup configuration.
See [store selection](README.md#where-memory-lives) for the full rules.

## Defaults and opt-ins

| Capability | Default | Configuration and scope |
|:--|:--|:--|
| Anchored Ask gate | On, including partial answers | `ask-gate.json`; identifiers must be supported by eligible evidence. [Answer states](README.md#how-ask-decides). |
| Lexical index and MaxScore | On for eligible reads | Local derived data; small abouts and unsupported query shapes use ordinary retrieval. `lexical-index.json` tunes limits; `KMP_LEXICAL_INDEX=off` disables the index. MaxScore pruning is always on with the index (its switch was removed at 1.0). [Index contract](../development/lexical-sidecar.md). |
| Calibrated confidence | Off | `confidence_calibration: "shipped"` in anchored `ask-gate.json`; only demotes `high` to `medium`. It does not certify correctness. [Confidence limits](README.md#how-ask-decides). |
| Ask re-ranking and focused Wake | Off | `rerank.json` or `wake-focus.json`, with working `typesafe.json`; sends selected text to TypeSafe Jev. [Configuration](../development/evidence-rerank.md). |
| Ask doubt band | Off | `ask-judge.json` with working `typesafe.json`; external judgments can veto citations. Promotion needs a separate opt-in. [Doubt band](README.md#the-doubt-band-opt-in-sends-text-to-typesafe). |
| Ask cross-language judge | Off | `ask-cross-language.json` with working `typesafe.json`; `veto` and `rescue` are each off unless named. Jev may veto an answer only the lexical bridge's translation carries, and rescue an unanchored UNKNOWN from a memory the bridge reached in another language (never `high`, marked `judged_by`). [Cross-language judge](README.md#the-cross-language-judge-opt-in-sends-text-to-typesafe). |
| Search expansions at write | Off | `write-expansions.json` with working `typesafe.json`; the writer proposes text and Jev judges it. Expansion-only matches stay outside the answer core. [Write expansions](README.md#search-expansions-at-write-opt-in-sends-text-to-typesafe). |
| Local semantic encoder | Off | `semantic-retrieval.json` points to a separately operated loopback encoder. Semantic-only proof cannot establish an answer or raise confidence. [Adapter contract](../development/semantic-retrieval.md). |

No external service is required for default embedded retrieval. Enabling a
TypeSafe integration sends the selected question or memory text needed by
that feature to the configured service; [Security and privacy](../security-privacy.md#typesafe-jev-opt-in-per-store)
lists what each one sends. A cloud agent also receives the evidence returned
to it under that host's data policy.

## Optional files beside the store

These are the twelve files `doctor` and `info` report under **Store config**.
Each goes in the data directory, beside `store/`. An absent file leaves its
default; a file that cannot apply is reported `rejected` and the default
stands. Restart the host after a change: a session reads them when it opens
its store.

| File | Default without it | What it does | Needs `typesafe.json` |
|:--|:--|:--|:--|
| `ask-gate.json` | Anchored gate on, with partial answers | `mode` (`anchored` or `off`), `partial`, `confidence_calibration`, `attribute_check`, `expansion_rescue_focus`. [How Ask decides](README.md#how-ask-decides). | No |
| `lexical-index.json` | Index limits as shipped | `max_candidate_share_percent`, `min_about_entries`, `head_window`, `continuation_chunk`. [Index contract](../development/lexical-sidecar.md). | No |
| `lexical-bridge.kmpb` | The machine table `<data home>/kmp/lexical-bridge.kmpb`, if any | A bridge table for this store only. `KMP_LEXICAL_BRIDGE` names another file or `none`. [Languages](languages.md). | No |
| `semantic-retrieval.json` | Off | Experimental: a local encoder on a loopback address. [Adapter contract](../development/semantic-retrieval.md). | No |
| `typesafe.json` | Off: nothing is sent | Opt this store into TypeSafe Jev: `{"endpoint":"https://api.typesafe.ai/v1/systemone","model":"jev-1.14.0","timeout_ms":20000}`. Only a pinned model is accepted. Turns on `kmp_curate` and `kmp_summaries_audit` judgements. `kmp-mcp doctor` asks TypeSafe whether the pinned model is still served (`GET /v1/models`, the key and no store text). | — |
| `rerank.json` | Off | Re-rank Ask candidates with Jev (`pool_size` 1–400 is required). [Configuration](../development/evidence-rerank.md). | Yes |
| `wake-focus.json` | Off | Focus a Wake with an `intent` through Jev. Same shape as `rerank.json`. | Yes |
| `ask-judge.json` | Off | The doubt band: Jev judges asks the gate settled in doubt. `{}` takes the defaults. [Doubt band](README.md#the-doubt-band-opt-in-sends-text-to-typesafe). | Yes |
| `ask-cross-language.json` | Off | Jev as the cross-language judge of Ask: `veto` (keep a translated citation only on a yes at `keep_at`, 0.7) and `rescue` (cite one memory the lexical bridge reached in another language for an unanchored UNKNOWN, at `rescue_at`, 0.9), each off unless named. [Cross-language judge](README.md#the-cross-language-judge-opt-in-sends-text-to-typesafe). | Yes |
| `write-relations.json` | Off | Jev proposes relations for memories just written. Each judgement it asks (up to three per write) waits at most 3 s; a late one is reported as not judged and the write stands. | Yes |
| `write-expansions.json` | Off | Keep writer-proposed search expansions that Jev accepts (it belongs to the memory at `accept_at`, 0.5, and adds no fact; its judgement waits at most 3 s, else not judged and not stored). [Write expansions](README.md#search-expansions-at-write-opt-in-sends-text-to-typesafe). | Yes |
| `judgement-book.json` | Verdict book on, 64 MiB | `max_bytes` (1 MiB – 4 GiB) for `judgements.sqlite3`, or `{"mode":"off"}`. It never turns judging on by itself. | Yes |
| `curate.json` | 120 facts per about for a review without focus | `partner_facts` (up to 512), `partner_filter`, `paths_corridor`. [Pairing orphans](README.md#how-a-review-without-focus-pairs-orphans). | Yes |

The TypeSafe key is read from `TYPESAFE_API_KEY`, else from the line
`TYPESAFE_API_KEY=…` in `$XDG_CONFIG_HOME/typesafe.env` (`~/.config/typesafe.env`
without `XDG_CONFIG_HOME`). That file is refused when group or others can read
it. Without `typesafe.json` the key is never read.

## The user config file

`$XDG_CONFIG_HOME/kmp/config.toml` (`~/.config/kmp/config.toml`) holds two
keys. Write them with `kmp-mcp config`, not by hand:

| Key | Values | Set with |
|:--|:--|:--|
| `memory_routing` | `on_request` (default), `always` | `kmp-mcp config memory-routing <on-request\|always>` |
| `memory_store` | an absolute path | `kmp-mcp config memory-store <absolute-path>`, `--clear` to remove |

An older file may hold an `ask_fallback_languages` line; it is ignored and
`doctor` says so.

## Environment switches

Every `KMP_*` variable kmp-mcp reads is in this table. The rules are the same
for all of them:

- A value kmp-mcp does not accept is **refused at startup**, naming the
  accepted values: the MCP server does not start, and every CLI verb except
  `info`, `doctor`, `--help` and `--version` exits 2. Nothing is read as its
  default any more (`KMP_KERNEL_GRPC_TLS_MODE=mutal` used to run plaintext).
- Words are case-insensitive. An on/off switch accepts `on`, `off`, `true`,
  `false`, `1`, `0`, `yes`, `no`.
- An empty or blank value means unset, except `KMP_VIEWER_ADDR`, where an
  empty value turns the viewer off.
- `kmp-mcp doctor` lists every switch the environment sets, fails on a refused
  value, and warns about a `KMP_*` name this build reads nowhere (a typo, a
  removed switch, another binary's variable).

| Name | Default | Accepted values | Effect |
|:--|:--|:--|:--|
| `KMP_MCP_DATA_DIR` | nearest project `.kernel/`, then the saved selection, then the per-user store | an absolute path; `~` and relative paths are refused | Which store opens, above every other rule. |
| `KMP_VIEWER_ADDR` | `127.0.0.1:7317`, or a free loopback port when it is busy | a loopback `host:port`; `off`, `none` or empty for no viewer | Where ChronoLoom listens. The `viewer` verb reads it the same way. |
| `KMP_MCP_OUTPUT_SCHEMAS` | off | on/off | `outputSchema` in `tools/list`. |
| `KMP_MCP_PASSAGES` | `inline` | `inline`, `shared` | Shared-passage tables in read results. |
| `KMP_LEXICAL_INDEX` | on | on/off | Whether asks are answered from the lexical index; `off` is the kill switch. |
| `KMP_LEXICAL_BRIDGE` | `<store>/lexical-bridge.kmpb`, then `<data home>/kmp/lexical-bridge.kmpb` | a file path, or `none` | The cross-language word table `kmp_ask` uses. |
| `TYPESAFE_API_KEY` | unset, then `typesafe.env` | a token of at most 512 characters | Key for TypeSafe Jev; read only when the store has `typesafe.json`. Its value is never printed. |
| `KMP_VERSION_CHECK` | on | on/off | Whether `plugin notice` asks GitHub for the latest release, at most once a day: the only network call KMP makes by default. |
| `KMP_PLUGIN_ROOT` | set by the plugin launchers | a directory | The plugin tree `guide sync` installs the longer guide from. |
| `KMP_INSTALL_DIR` | `~/.local/bin` | a directory | Default of `setup`/`update --engine-dir`. |
| `KMP_MCP_BACKEND` | `embedded`; `grpc` when `KMP_KERNEL_GRPC_ENDPOINT` is set | `embedded`, `grpc` | Which kernel answers. gRPC is experimental and outside the 1.0 promise. |
| `KMP_KERNEL_GRPC_ENDPOINT` | unset | a URL; `https://` implies server TLS | Selects the remote gRPC kernel (experimental). Under it the commands that read the local store refuse to run. |
| `KMP_KERNEL_GRPC_TLS_MODE` | inferred: `mutual` with a cert or key path, `server` with a CA path, domain name or `https://` endpoint, else `disabled` | `disabled`/`plaintext`, `server`/`tls`, `mutual`/`mtls` (the words `kmp-server` accepts) | TLS to the gRPC kernel (experimental). |
| `KMP_KERNEL_GRPC_TLS_CA_PATH`, `_CERT_PATH`, `_KEY_PATH`, `_DOMAIN_NAME` | unset | paths; a host name | TLS material for the gRPC kernel (experimental). |

Removed at 1.0: `KMP_MCP_ENGINE` (it could only say `sqlite`), the
`KMP_MCP_BACKEND` aliases `live` and `fixtures`, and `KMP_LEXICAL_MAXSCORE`
(MaxScore is always on). Setting one of them changes nothing; `doctor` names
it as unread.

**Test and evaluation hooks** are not part of a release binary: the `fixture`
backend, `KMP_LEXICAL_INDEX=shadow|verify`, `KMP_TYPESAFE_CASSETTE` and
`KMP_TYPESAFE_CASSETTE_MODE`, `KMP_EVAL_PARTNER_FACTS`,
`KMP_JUDGEMENT_DEADLINES` and `KMP_LATEST_VERSION` exist only in debug builds
(what `cargo test` runs) and in builds with the `eval-hooks` cargo feature
(what the memory bench builds). On a release binary the variables do nothing,
and `fixture`, `shadow` and `verify` are refused like any unknown value.

## Read the diagnostics

The **Store config** section of `info` and `doctor` runs the same loaders a
starting server runs:

| State | Meaning | Next step |
|:--|:--|:--|
| `on` | The file loaded; the report lists effective settings. | Compare those settings with the intended configuration. |
| `on, with its defaults` | The feature loaded with a warning and fallback settings. | Read the reason and correct the affected setting. |
| `rejected` | A present optional configuration could not apply. | Repair the reported syntax, value or dependency; restart and recheck. |
| `off (absent)` | The optional file is absent. | The feature keeps its default; the anchored gate and lexical index can still be on. |

These diagnostics read what a new process would load. They do not prove that
an already-running host has reloaded the files, that its MCP connection is
live, or that an external service will answer its next request. The startup
logs also record configuration verdicts as `kmp_store_config`.

The executable sources are the [configuration loaders](../../crates/kmp-mcp/src/serving/adapters/store_config_loads.rs)
and [diagnostic renderer](../../crates/kmp-mcp/src/lifecycle/adapters/store_config_probe.rs).
For tools missing from the host, use the [missing-tools runbook](../runbooks/mcp-tools-missing.md).
