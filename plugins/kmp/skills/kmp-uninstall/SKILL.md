---
name: kmp-uninstall
description: Preview or perform KMP removal while protecting memory and respecting plugin versus standalone ownership.
---

# KMP uninstall

Run `kmp-mcp uninstall` as a dry run first. Never add `--apply` or `--purge`
unless the user explicitly requested it; `--purge` skips the protective export.
Name the export path and restore command. Remove host wiring only from its
owner: `codex plugin remove` for plugin-managed installs, or the global MCP and
standalone assets for standalone installs. Do not remove both paths when only
one owns the installation.
