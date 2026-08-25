---
name: kmp-save
description: Checkpoint this project's KMP memory into its maintained repository bundle and show the diff.
---

# KMP save

Run `kmp-mcp export`, which maintains `.kmp/memory.jsonl` for a project-scoped
store. Show `git diff --stat .kmp/memory.jsonl` and the relevant diff, then
summarize new decisions and their reasons in prose. Do not stage or commit the
bundle unless the user asks. Flag a non-append-only diff.
