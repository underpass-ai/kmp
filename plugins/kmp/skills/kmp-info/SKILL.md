---
name: kmp-info
description: Show the installed KMP version, selected store, engine, durability, tools, and viewer. Use for identity and store-selection questions, not diagnosis.
---

# KMP info

Run `kmp-mcp info`. Show its first branded block verbatim, then report which
store was selected and which `chosen by:` rule selected it. Include the
durability verdict and viewer URL. Mention the backend only when it is not the
default embedded backend; always call out `fixture` because it stores nothing.

If the user is troubleshooting, route to the `kmp-doctor` skill instead of
diagnosing from this identity report.
