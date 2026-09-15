# Surface and marketplace acceptance, 15 September 2026

## Result

Neither #544 nor #736 is closed by this audit. It completes a current transport
inventory and eight paired authored journeys for #544, and identifies the exact
local source matching the stale #736 card. The remaining host and installation
checks below are concrete acceptance work, not inferred successes.

All work started from freshly fetched main
`fc352d7d37ac0480cc79120a6288bfd446fa4c86`. The original `verify/claims`
checkout was preserved. Captures, failures, lossless compressed RPC traces and
hashes are in [the evidence directory](../../artifacts/acceptance-544-736-20260915/).

## #544: surface and complete authored journeys

The main binary SHA-256 is
`455820c837eb3bf15e8c2d411a6726ea2418f6f682ebf4c7d2c8a6dd7a636801`.
The installed 0.18.5 binary SHA-256 is
`c24712dfcc44c2293d4fb520ff8eb7f91ebb0b8c5a7de52aafc9c4c5caa2c910`.
Main was built in the development profile; the installed release has a different
build profile. Their wall times are recorded but are not a performance comparison.

`surface-metrics-validated.json` separates native initialization, catalogue, MCP Apps
resources, guide entry/editorial/skills, actual host declarations, and guide and
memory RPCs within each journey. It uses tiktoken 0.14.0, `o200k_base`, compact
UTF-8 JSON per envelope and exact text for files. These representations are not
billed context. Resources sent to an Apps client are not assumed to be visible
to the model. All model-call counts in this deterministic harness are zero.

The fresh main native initialization response is 1,705 bytes / 357 tokens; its
complete tools/list response is 247,046 bytes / 51,250 tokens. The older merge
comparison's 51,295 → 51,236 counts refer to the catalogue object without the
JSON-RPC response envelope. Those measurements use different representation
boundaries and are both retained.

| Journey | RPCs per variant | Installed request + response tokens | Main request + response tokens |
| --- | ---: | ---: | ---: |
| Semantic batch writing | 40 | 115,904 | 115,855 |
| Decision history and recovery | 47 | 178,512 | 178,572 |
| Four clocks and dated reads | 64 | 136,423 | 136,421 |
| Budgeted proof and interval navigation | 77 | 240,615 | 240,521 |
| Dimensional memberships | 27 | 97,543 | 97,447 |
| Distributed incident and cross-about evidence | 52 | 138,079 | 138,050 |
| Workflow proof audit | 51 | 135,927 | 135,927 |
| Shared resumption and view revision | 69 | 235,155 | 235,094 |

All sixteen runs pass their shipped source-backed behavioral checks, including
explicit continuations and retained expected errors. Small token differences
also contain run-specific IDs and clocks; do not attribute them to compression.
The shared-view browser gesture is explicitly simulated by the existing replay,
not a human review. Each manifest records complete commands, wall time, binary
identity and hashes of the original uncompressed files. Startup/catalogue, guide,
memory, partial replies and refusals are separated in the metrics.

Ownership remains defined in [agent-surface.md](agent-surface.md): common routing
and trust boundaries in initialize, move-specific contracts in schemas, selective
instruction bodies in editorial.json, generated guide assets, and a brief skill
router. Fileless hosts can use exact guide refs. No new editorial gate or public
tool change is introduced here.

### Remaining #544 acceptance

The current desktop task's real `functions.ALL_TOOLS` capture still contains
**13 opaque argument declarations**. Its running installed engine predates the
merged conditional-schema change; a compiled candidate does not refresh that
task. Native candidate schemas are captured, but a refreshed desktop host
declaration is **not** claimed.

1. Load the final candidate through a supported installation/reload in a new
   desktop session and capture the same 13 argument declarations. Verify fields
   are named and callable, counting any newly exposed text.
2. Complete fresh-agent validation on that actual surface, with full
   continuations, source/qualifier/clock fidelity and no unnecessary previews.
   The separate #683 fresh writer is evidence only for its recorded driver and
   source. It cannot prove the desktop host fix or causal improvement.
3. Keep the full preparation/example/read costs when assessing savings; the
   current paired replays establish behavioral coverage, not billing or learning.

Reproduction:

```bash
python3 scripts/performance/agent_surface_capture.py --binary /path/to/kmp-mcp --output /new/native.json.gz
python3 scripts/performance/acceptance_surface_journeys.py --binary /path/to/kmp-mcp --output /new/journeys
uv run --with tiktoken==0.14.0 python scripts/performance/surface_acceptance_metrics.py --evidence artifacts/acceptance-544-736-20260915 --main-journeys journeys-main-validated --native-main native-main-validated.json.gz --output surface-metrics-validated.json
```

### Build and integrity review

The first runner's `main_base` recorded the harness checkout, not the binary's
build provenance. Its original eight candidate runs and metrics are preserved
under `journeys-main`/`surface-metrics.json` and are not silently relabelled.
Review prompted a new build validation with the build worktree's commit/tree,
exact command, Cargo/Rust versions and workspace-profile hash. The crates,
plugin assets and Cargo build inputs are equal to the fresh main base before
and after that validation. A repeated build preserves the binary SHA-256 in
`main-build-provenance.json`, bound to all eight `journeys-main-validated` runs
and the fresh native capture. The table above uses this validated rerun.

The first validation attempt changed the selected target executable hash despite
returning success; its stdout/stderr are retained. We do not infer a runtime
behavior change from that hash difference. A second stable build plus new
captures establish the identity used for the final measurements. The initial
writer experiment remains separately bound to its original 8ff8ca32 binary.

The metrics tool now verifies every original file's length and SHA-256 before
counting, classifies opaque guide continuations with their originating response,
and counts shortened cores as partial. A tampered compressed trace is refused
by the recorded integrity check. No original capture was modified by that test.

## #736: the legacy source is identified

A fresh Codex CLI 0.154.0 app-server `plugin/list` against the existing local
configuration returned both records:

| Layer | Legacy entry | Maintained entry |
| --- | --- | --- |
| Identity | `kmp@personal` | `kmp@underpass` |
| Catalogue | `~/.agents/plugins/marketplace.json` | configured Underpass git marketplace |
| Source | `/home/gx10a/plugins/kmp` | `https://github.com/underpass-ai/kmp.git`, ref `main` |
| Version in source | `0.1.11+codex.20260818222635` | `0.18.5` |
| State reported | not installed | installed and enabled |
| Website before this audit | missing | `https://underpassai.com/` |

The personal source manifest exactly matches the screenshot's version, display
name and missing website. This establishes the source of a matching stale record
on this host; the historical screenshot alone did not establish that attribution.
The maintained cached checkout is `4bf45ac8f50121770a65ea375338c16a287fe9d5`.
The running installed executable is separately identified by SHA-256 in
`runtime-identity.json`; neither catalogue version nor cached files alone prove it.

The local legacy manifest was reversibly clarified to **KMP legacy local
(0.1.11)** with a maintained-source explanation and the official website. Its
version remains byte-for-byte `0.1.11+codex.20260818222635`; no cachebuster masks
the defect. Before/after manifests are preserved and validation passed. A fresh
app-server catalogue read confirms the new label alongside `kmp@underpass`
0.18.5. No plugin was installed/uninstalled, no cache was deleted, no engine was
replaced and no store was migrated by this correction.

This is a local source correction, not a repository manifest release. The
personal source owner controls that entry. Underpass maintainers own the
maintained repository and release metadata; Codex loads/presents those records
and the user selects the executable/store through binary setup. The earlier
external-publisher hypothesis is not required to explain this local duplicate.
The unrelated KMP/MADE shared-marketplace-name hypothesis remains unproven.

### Remaining #736 acceptance

1. Reopen the desktop Plugins view and verify the two distinct labels and links.
   App-server catalogue data is captured; an actual refreshed desktop screenshot
   has not been captured in this task.
2. From the maintained card, validate a genuinely fresh install and an existing
   user's update, then reload and record the MCP process's executable hash/version
   and unchanged selected-store identity/content. Existing installed state is not
   evidence of either installation experiment.
3. Retain those before/after records and close only when the maintained card's
   install action is verified. No external publisher action is currently shown
   to be necessary for the identified personal entry.

The [official plugin documentation](https://learn.chatgpt.com/docs/plugins)
describes marketplace-based discovery and the new-session boundary for installed
skills/tools. That lifecycle guidance does not substitute for these missing runs.
The public plugin-directory search returned no KMP match; it is a separate
catalogue from the locally configured marketplace.

```bash
python3 scripts/performance/kmp_marketplace_capture.py --output /new/kmp-catalogue.json
```

The capture command is read-only and refuses to overwrite its output.
