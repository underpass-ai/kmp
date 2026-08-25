---
name: kmp-setup
description: Install, update, and configure KMP for Codex. Use for first setup, upgrades, ownership repair, or Ask fallback-language configuration.
---

# KMP setup

Diagnose first with `kmp-mcp doctor`. Resolve the plugin root as two directories
above this `SKILL.md`.

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
time and never enter this fallback. Upgrades must leave this policy intact.

Finish by rerunning the doctor. A running Codex session needs one restart to
load changed skills or MCP wiring.
