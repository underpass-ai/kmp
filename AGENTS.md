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
cargo run --locked --quiet -p kmp-release -- readme sync
```

Do not hand-edit the corresponding marked blocks in the other two files.
Release preparation synchronizes them and CI rejects both byte drift and a
missing product-contract claim, but the generated changes must still be
reviewed and committed together.

## Guide parity

The public READMEs explain the product; the installed guides teach it. A
change to KMP's tools, verb semantics, storage model, clocks, viewer or setup
must update `plugins/kmp/guide/editorial.json` in the same change and regenerate
both guide abouts with:

```bash
cargo run --locked --quiet -p kmp-release -- guide assets write --binary target/debug/kmp-mcp
```

`guide:kmp-agent` is the exact operational guide for agents. `guide:kmp` is a
shorter human path opened visually through `open:guide`; do not collapse them
into one audience. Setup, update, the three public READMEs and both guides must
describe the same shipped version before a release is prepared. The supported
`scripts/release.sh version` verb builds the bumped engine and regenerates both
guide assets; candidate and tag paths reject a stale guide envelope.
