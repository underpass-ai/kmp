---
name: kmp-revert
description: Supersede a KMP decision without deleting history. Use when the user asks to undo or reverse a stored decision.
---

# KMP revert

Resolve and confirm the exact target ref before writing. Inspect it and check
whether it is already superseded. Record the new current decision with
`kmp_write_memory` and a `supersedes` relation whose `why` explains why the old
decision stopped being right and whose evidence proves that rationale. Then
show the new state and rewind to prove the old state still exists. Never look
for or simulate deletion.
