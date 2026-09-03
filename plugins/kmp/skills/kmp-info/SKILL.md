---
name: kmp-info
description: Show the installed KMP version, selected store, engine, durability, tools, and viewer. Use for identity and store-selection questions, not diagnosis.
---

# KMP info

Run `kmp-mcp info`. Show its first branded block verbatim, then report which
store was selected and which `chosen by:` rule selected it. Include the
durability verdict, the viewer URL and the `lexical bridge` line, which says
whether Ask crosses languages inside the kernel on this store or matches
within one language until a table is installed. Mention the backend only when it is not the
default embedded backend; always call out `fixture` because it stores nothing.

If the user is troubleshooting, route to the `kmp-doctor` skill instead of
diagnosing from this identity report.
