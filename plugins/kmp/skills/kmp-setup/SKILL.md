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
the same release. An ordinary update resolves the latest public release in
the native lifecycle; `--version <release>` selects an explicit target.
The installed plugin's manifest only selects the initial bootstrap engine
and the default setup version. Do not update only one host or one half.

Once the release is proved and every host points at it, the update names the
superseded versions each host's plugin cache still holds, leaving the installed
release and the one before it for rollback. It removes none of them: a session
that was already open dispatches its skills out of the version directory it
started in, and deleting that directory breaks the session until its host
restarts. The next time the engine starts, that start removes them. The receipt
names what is still there under `plugin_caches` as `deferred`; report it as
deferred rather than as freed space.

For an enabled Codex plugin, the plugin owns MCP. Install or update the engine
with `<plugin-root>/scripts/kmp-install-binary.sh`, but do not add a global
`mcp_servers.kmp` registration, copied prompts, or an AGENTS snippet. If a
global registration also exists, report the collision and remove it only as
the explicit ownership repair for the requested setup.

Standalone Codex wiring is retired. The native plugin is the single MCP owner;
reject `--standalone` and diagnose any old global registration as a collision.

Show the active configuration with `kmp-mcp config`. It reports the agent
policy and the user memory selection, and starts no host.

`memory store` is which memory KMP opens. Selection runs in one order:
`KMP_MCP_DATA_DIR`, then the saved selection, then the nearest project root,
then the per-user default. `kmp-mcp config` prints the saved selection, the
effective one, and which of those rules won, so a workspace that is not a
repository never reaches a store nobody chose without saying so.

Save a selection with `kmp-mcp config memory-store <absolute-path>`, and undo
it with `kmp-mcp config memory-store --clear`. The selection is one line in
the user config file, so it survives a normal desktop restart with no
environment variable and no launcher script. It creates no store: the first
write does. A relative or `~` path is refused, because a saved selection has
to mean one directory from every working directory.

A directory holding memory this engine cannot open is refused with the reason
and the repair. Never present that as a reason to migrate: KMP does not
migrate, move, convert or overwrite an existing store, and setup and update
must not select or write one. Selecting a fresh store is the user's explicit
decision, and the old store stays exactly where it is.

`memory routing` decides whether an agent enters KMP unasked. The default is
`on request`: memory is called when the user asks for KMP, when a kmp skill or
command runs, or when the project opts in. Change it only when the user asks
for always-on recall, with `kmp-mcp config memory-routing always`;
`on-request` returns to the default. Never turn it on as part of an install.

There is no language setting to configure. A semantic question is asked in
English with the user's own words passed as `asked_as`, and the answer is
given in the user's language with stored evidence, refs, relation `why` and
source metadata preserved byte-for-byte. `kmp-mcp doctor` and `kmp-mcp info`
name the lexical-bridge table a store would read, or say there is none and
what that means for Ask; report that line when the user asks about
languages. An older config file may still carry an `ask_fallback_languages`
line; it is ignored and the doctor says so. Questions in Chinese, Japanese or
Thai are not segmented by word yet; storage remains byte-exact. Upgrades must
leave the policy file intact.

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
