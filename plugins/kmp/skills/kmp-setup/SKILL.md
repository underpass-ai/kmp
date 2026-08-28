---
name: kmp-setup
description: Install, update, and configure KMP for Codex. Use for first setup, upgrades, ownership repair, or Ask fallback-language configuration.
---

# KMP setup

Resolve the plugin root as two directories above this `SKILL.md`. Diagnose
first with `<plugin-root>/scripts/kmp-doctor.sh`; it includes host ownership
and effective wiring checks that the process-local `kmp-mcp doctor` cannot
see. Fall back to `kmp-mcp doctor` only if the bundled script is unavailable,
and say that host ownership was not checked.

When a session notice or version comparison says a newer release exists, run
`<plugin-root>/scripts/kmp-update.sh --codex`. That updates the native Codex
plugin and installs the checksummed engine from the same release. Do not update
only one half.

For an enabled Codex plugin, the plugin owns MCP. Install or update the engine
with `<plugin-root>/scripts/kmp-install-binary.sh`, but do not add a global
`mcp_servers.kmp` registration, copied prompts, or an AGENTS snippet. If a
global registration also exists, report the collision and remove it only as
the explicit ownership repair for the requested setup.

Standalone Codex wiring is an advanced, explicit mode:
`install-kmp-plugin.sh --codex --standalone`. Refuse that mode while a KMP
plugin is enabled.

Show the active semantic-Ask fallback policy with `kmp-mcp config`. Change it
with `kmp-mcp config ask-fallback-languages <comma-separated-tags>` when the
user requests a different list; `none` disables retries. With no config, one
English retry is active by default. Explain that only a semantic query may be
translated: answer in the user's language and preserve stored evidence, refs,
relation `why`, and source metadata byte-for-byte. Temporal requests navigate
time and never enter this fallback. Chinese, Japanese, and Thai fallback tags
are rejected until Ask supports word segmentation for those scripts; storage
remains byte-exact. Upgrades must leave this policy intact.

Finish by rerunning `<plugin-root>/scripts/kmp-doctor.sh`. A running Codex
session needs one restart to load changed skills or MCP wiring.
