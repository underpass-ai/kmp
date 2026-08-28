# KMP repository working agreement

## Public README parity

KMP has three public README surfaces and a change to the product overview must
update all three in the same change:

- `README.md` — GitHub repository;
- `plugins/kmp/README.md` — Codex and Claude plugin marketplaces;
- `crates/kmp-mcp/README.md` — crates.io.

The marked `kmp:public-overview` block in `plugins/kmp/README.md` is canonical.
It carries the common product contract: local-first SQLite memory, decisions
and evidence rather than transcripts, Codex and Claude Code, and the ten
memory plus three semantic view tools over shared ChronoLoom. After editing
it, run:

```bash
python3 scripts/release/sync-public-readme.py sync
```

Do not hand-edit the corresponding marked blocks in the other two files.
Release preparation synchronizes them and CI rejects both byte drift and a
missing product-contract claim, but the generated changes must still be
reviewed and committed together.
