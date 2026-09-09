# KMP repository working agreement

## MCP simplification integration

The active MCP evolution follows solution A: simplify the existing native MCP
contract. SQL, GraphQL and another query language are out of scope for this track.

- Base evolution branches on `integration/mcp-simplification` and target their
  pull requests at that branch. Do not target `main` or `work/sota-gaps` for this
  redesign. Pass the PR base explicitly; the repository default remains `main`.
- This is a breaking redesign. Backward compatibility, legacy-ref adapters and
  old-store migration are not requirements. Reject unsupported formats explicitly.
- Implement in this order: dimensions; semantic writing and actionable feedback;
  reading and continuations; independent agent evaluation.
- Keep changes reviewable and run the behavioral checks appropriate to each
  change. Do not add editorial CI gates or change repository protections.
- Bugs in released behavior retain the normal bug PR and check process against
  `main`; bring the fix into integration when needed. Bugs specific to new
  integration behavior target integration, with the same bug discipline.
- Integrating an evolution PR does not publish a release or merge this track
  into `main`. The final integration-to-main PR is a separate delivery step.

See [the integration plan](docs/development/mcp-simplification.md) for scope,
compatibility and acceptance criteria. This section applies to this evolution;
existing guide and surface maintenance procedures still apply.

## Public README parity

KMP has three public README surfaces and a change to the product overview must
keep all three consistent:

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
Release preparation synchronizes them. Review the generated changes together;
wording and editorial organization are documentation guidance, not CI gates.

## Guide parity

The public READMEs explain the product; the installed guides teach it. A
change to KMP's tools, verb semantics, storage model, clocks, viewer or setup
must update `plugins/kmp/guide/editorial.json` in the same change and regenerate
both guide abouts with:

```bash
cargo run --locked --quiet -p kmp-release -- guide assets write --binary target/debug/kmp-mcp
```

`guide:kmp-agent` is the exact operational guide for agents. The same generation
writes `plugins/kmp/guide/AGENT.md`: a brief entry with exact references to
extended verb guidance and examples, without loading a second full manual.
Do not edit this generated Markdown separately from its editorial source.
`guide:kmp` is a
shorter human path opened visually through `open:guide`; do not collapse them
into one audience. Setup, update, the three public READMEs and both guides must
describe the same shipped version before a release is prepared. The supported
`scripts/release.sh version` verb builds the bumped engine and regenerates both
guide assets; candidate and tag paths reject a stale guide envelope.

## Agent-facing surface maintenance

When adding or changing a tool, argument, result, routing rule, skill, guide
or example, follow [the surface maintenance procedure](docs/development/agent-surface.md).
It explains the canonical owner, generated assets, behavioral verification,
context-cost measurement and delivery evidence for humans and agents. Editorial
recommendations remain informative; do not add CI checks for their wording,
length or file placement.
Keep the entry brief, make extended verb guidance discoverable, and verify
what the host actually exposes. Update that procedure when its workflow changes.
