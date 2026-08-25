---
name: kmp-restore
description: Restore a project's KMP memory from its committed .kmp/memory.jsonl bundle into an empty store.
---

# KMP restore

Run `kmp-mcp import`. Restore requires an empty destination; if it refuses,
stop and offer either keeping local memory or importing into a separate
`KMP_MCP_DATA_DIR`. Never delete a store to make restore pass. After success,
report the event count and use `kmp_wake` on the restored about when the live
tools are available.
