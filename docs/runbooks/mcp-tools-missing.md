# Runbook: MCP tools are missing

Use this when the agent cannot see KMP, the MCP process fails to start, or the
tool inventory is stale.

## 1. Diagnose without changing anything

Run the host-aware plugin workflow:

```text
kmp-doctor
```

For Claude Code the command is `/kmp:doctor`. If the plugin workflow is not
available, fall back to:

```bash
kmp-mcp doctor
```

The fallback cannot inspect plugin/global ownership inside every host.

## 2. Classify the failure

| Finding | Meaning | Repair |
|:--|:--|:--|
| Binary missing | Plugin exists but cannot launch its engine | Run `kmp-setup`; restart the host. |
| Plugin plus global MCP registration | Two owners can select different stores or binaries | Keep the plugin owner and remove the duplicate registration through setup repair. |
| `kernel-memory` registration | Retired server identity or transport syntax | Remove the retired registration; let the plugin own `kmp`. |
| Ten tools in a standalone smoke, none in the current session | The host cached its startup inventory | Restart the host once. |
| Fixture backend | Responses are canned and nothing is stored | Unset `KMP_MCP_BACKEND=fixture`. |
| gRPC without endpoint | Remote backend was requested with no kernel | Set a verified endpoint or return to embedded mode. |
| Legacy redb lock | Another process owns the old single-process store | Stop the other process or migrate it with the embedded recovery runbook. |

## 3. Verify the repair

Rerun the host-aware doctor. A healthy result must identify one MCP owner, one
binary, the intended backend and store, and exactly ten live tools.

If setup changed plugin files or MCP wiring, restart the host before judging
the result from an already-running session.
