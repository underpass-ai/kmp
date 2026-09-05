---
name: kmp-uninstall
description: Preview or perform KMP removal while protecting memory and respecting plugin versus standalone ownership.
---

# KMP uninstall

Run `kmp-mcp uninstall` as a dry run first. It is the unscoped command and it
proposes the whole installation, so never reach for it to remove one thing.

To retire one memory, use `kmp-mcp uninstall --store <absolute-path>`; to
remove one engine — the superseded copy doctor warns about — use
`kmp-mcp uninstall --engine <absolute-path>`. Either way the preview and the
apply cannot reach any other store, engine, plugin or host wiring. The selected
path must be an existing store or an engine named `kmp-mcp`. Never add
`--apply` or `--purge` unless the user explicitly requested it; `--purge` skips
the protective export.

An apply must refuse an active store and leave every process running. Tell the
user which owning host must be stopped or restarted; never kill it on behalf of
uninstall. Retry only after that host releases the store. Name the export path
and restore command.

The preview distinguishes `held` from `kept`. `held` names a host and process
still reading that piece: it is ours, it is a restart away, and uninstall will
not end the process to get at it. `kept` is final here — the piece was never
this verb's to remove. Report the two differently; a reader told only that
something was kept has no next step.

Lines labelled `leftover` are what the retired standalone wiring left behind —
Codex `/kmp-` prompts and the shell scripts beside a standalone engine. Nothing
running reads them. They are safe to remove even while the plugin installation
stays exactly as it is.

Use the unscoped command only when the user requested removal of the whole KMP
installation. Remove host wiring only from its owner: `codex plugin remove` for
plugin-managed installs, or the global MCP and standalone assets for
standalone installs. Do not remove both paths when only one owns the
installation.
