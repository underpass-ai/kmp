# Store configuration and retrieval defaults

This page describes the current `main` branch. The retrieval defaults below
ship in v0.24.0; the **Store config** section in `info` and `doctor` was added
after that release. Check `kmp-mcp --version` before comparing an installed
engine with these docs.

## Select the memory first

Run these commands from the host's working directory with the same environment:

```bash
kmp-mcp config
kmp-mcp info
kmp-mcp doctor
```

`KMP_MCP_DATA_DIR` wins over the saved `config memory-store` selection, which
wins over the nearest Git root's `.kernel/`, then the per-user default.
Optional JSON files go in that selected data directory, beside `store/`,
not inside it. Restart the MCP process after changing startup configuration.
See [store selection](README.md#where-memory-lives) for the full rules.

## Defaults and opt-ins

| Capability | Default | Configuration and scope |
|:--|:--|:--|
| Anchored Ask gate | On, including partial answers | `ask-gate.json`; identifiers must be supported by eligible evidence. [Answer states](README.md#how-ask-decides). |
| Lexical index and MaxScore | On for eligible reads | Local derived data; small abouts and unsupported query shapes use ordinary retrieval. `lexical-index.json` tunes limits; `KMP_LEXICAL_INDEX=off` disables the index and `KMP_LEXICAL_MAXSCORE=off` disables pruning. [Index contract](../development/lexical-sidecar.md). |
| Calibrated confidence | Off | `confidence_calibration: "shipped"` in anchored `ask-gate.json`; only demotes `high` to `medium`. It does not certify correctness. [Confidence limits](README.md#how-ask-decides). |
| Ask re-ranking and focused Wake | Off | `rerank.json` or `wake-focus.json`, with working `typesafe.json`; sends selected text to TypeSafe Jev. [Configuration](../development/evidence-rerank.md). |
| Ask doubt band | Off | `ask-judge.json` with working `typesafe.json`; external judgments can veto citations. Promotion needs a separate opt-in. [Doubt band](README.md#the-doubt-band-opt-in-sends-text-to-typesafe). |
| Search expansions at write | Off | `write-expansions.json` with working `typesafe.json`; the writer proposes text and Jev judges it. Expansion-only matches stay outside the answer core. [Write expansions](README.md#search-expansions-at-write-opt-in-sends-text-to-typesafe). |
| Local semantic encoder | Off | `semantic-retrieval.json` points to a separately operated loopback encoder. Semantic-only proof cannot establish an answer or raise confidence. [Adapter contract](../development/semantic-retrieval.md). |

No external service is required for default embedded retrieval. Enabling a
TypeSafe integration sends the selected question or memory text needed by
that feature to the configured service. A cloud agent also receives the
evidence returned to it under that host's data policy.

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
