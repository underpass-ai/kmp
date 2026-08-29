---
name: kmp-uninstall
description: Preview or perform KMP removal while protecting memory and respecting plugin versus standalone ownership.
---

# KMP uninstall

Run `kmp-mcp uninstall` as a dry run first. To retire one memory, use
`kmp-mcp uninstall --store <absolute-path>` so the preview and apply cannot
reach any other store, engine, plugin or host wiring. The selected path must be
an existing KMP store. Never add `--apply` or `--purge` unless the user
explicitly requested it; `--purge` skips the protective export.

An apply must refuse an active store and leave every process running. Tell the
user which owning host must be stopped or restarted; never kill it on behalf of
uninstall. Retry only after that host releases the store. Name the export path
and restore command.

Use the unscoped command only when the user requested removal of the whole KMP
installation. Remove host wiring only from its owner: `codex plugin remove` for
plugin-managed installs, or the global MCP and standalone assets for
standalone installs. Do not remove both paths when only one owns the
installation.
