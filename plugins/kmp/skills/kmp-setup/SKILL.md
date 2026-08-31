---
name: kmp-setup
description: Install, update, and configure KMP for Codex. Use for first setup, upgrades, ownership repair, or Ask fallback-language configuration.
---

# KMP setup

Resolve the plugin root as two directories above this `SKILL.md`. Diagnose
first with `<plugin-root>/scripts/kmp-doctor.sh`; it is a thin adapter into
`kmp-mcp doctor`, whose Rust lifecycle diagnosis owns host inventory, engine
proof, effective wiring and plugin-tree parity.

When a session notice or version comparison says a newer release exists, run
`<plugin-root>/scripts/kmp-update.sh`. That inventories both native hosts,
updates every installed KMP plugin, and installs the checksummed engine from
the same release. Do not update only one host or one half.

Once the release is proved and every host points at it, the update removes the
superseded versions each host's plugin cache had kept, leaving the installed
release and the one before it for rollback. The receipt names what went under
`plugin_caches`; report it rather than letting sixty megabytes disappear
quietly.

For an enabled Codex plugin, the plugin owns MCP. Install or update the engine
with `<plugin-root>/scripts/kmp-install-binary.sh`, but do not add a global
`mcp_servers.kmp` registration, copied prompts, or an AGENTS snippet. If a
global registration also exists, report the collision and remove it only as
the explicit ownership repair for the requested setup.

Standalone Codex wiring is retired. The native plugin is the single MCP owner;
reject `--standalone` and diagnose any old global registration as a collision.

Show the active semantic-Ask fallback policy with `kmp-mcp config`. Change it
with `kmp-mcp config ask-fallback-languages <comma-separated-tags>` when the
user requests a different list; `none` disables retries. With no config, one
English retry is active by default. Explain that only a semantic query may be
translated: answer in the user's language and preserve stored evidence, refs,
relation `why`, and source metadata byte-for-byte. Temporal requests navigate
time and never enter this fallback. Chinese, Japanese, and Thai fallback tags
are rejected until Ask supports word segmentation for those scripts; storage
remains byte-exact. Upgrades must leave this policy intact.

Setup and update must not select or write a memory store. Guide sync is a
separate, explicit data operation. Run it only when the user asks to install
or refresh the guides:

```bash
KMP_MCP_BIN=<path-to-kmp-mcp> <plugin-root>/scripts/kmp-guide-sync.sh sync
```

It writes `guide:kmp-agent` and `guide:kmp` into the store selected by that
command. In a project, that is project memory and its commit-native bundle.
Name this effect before running it. Exact reruns add no events; a later plugin
version supersedes only changed guide entries.

Finish by rerunning `<plugin-root>/scripts/kmp-doctor.sh`. A running Codex
session needs one restart to load changed skills or MCP wiring.
