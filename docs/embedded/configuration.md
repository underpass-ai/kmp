# Store configuration and retrieval defaults

This page describes the current `main` branch. The retrieval defaults below
ship in v0.24.0; the **Store config** section in `info` and `doctor` was added
after that release, and the **Environment** section and strict switches in
1.0. Check `kmp-mcp --version` before comparing an installed
engine with these docs.

## Select the memory first

Run these commands from the host's working directory with the same environment:

```bash
kmp-mcp config
kmp-mcp info
kmp-mcp doctor
```

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
| Search expansions at write | Off | `write-expansions.json` with working `typesafe.json`; the writer proposes text and Jev judges it. Expansion-only matches stay outside the answer core. [Write expansions](README.md#search-expansions-at-write-opt-in-sends-text-to-typesafe). |
| Local semantic encoder | Off | `semantic-retrieval.json` points to a separately operated loopback encoder. Semantic-only proof cannot establish an answer or raise confidence. [Adapter contract](../development/semantic-retrieval.md). |

No external service is required for default embedded retrieval. Enabling a
TypeSafe integration sends the selected question or memory text needed by
that feature to the configured service. A cloud agent also receives the
evidence returned to it under that host's data policy.

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
| `KMP_PLUGIN_ROOT` | set by the plugin launchers | a directory | The plugin tree the engine seeds its guide from. |
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

Current `main` uses the startup configuration loaders for the **Store config**
section in both `info` and `doctor`:

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
