# KMP pull-request evolution record

This catalog records every merged pull request in the complete KMP code
lineage: the archived `underpass-ai/rehydration-kernel` repository followed
by the current `underpass-ai/kmp` repository. Reused PR numbers are kept in
separate directories and are never conflated.

## Coverage

- Latest merged PR in snapshot: `2026-08-18T19:19:40Z`
- Audited `main` tip: `eae7cdee3c836b51f1f8797ca81459ce0b557efd`
- Initial foundation documents: 1
- Merged pull requests documented: 209
- Rehydration Kernel (archived predecessor): 143 merged PRs
- KMP: 66 merged PRs

GitHub issues and pull requests share a numeric sequence. Missing PR
numbers are therefore not gaps in this catalog: no Markdown file is
invented when no merged pull-request object exists.

## Evidence policy

1. GitHub is authoritative for PR identity, author, description, state,
   timestamps, refs, and merge SHA.
2. The local Git graph is authoritative for reachability, first-parent
   integration ranges, commits, paths, and line statistics.
3. Feature classification is conservative. Positive removal claims require
   removal language in the PR title or integrated commit subjects.
4. The full PR description is retained next to verified tree evidence so a
   reviewer can distinguish author intent from repository fact.

Start with [the initial foundation](./0000-initial-foundation.md), then
follow the tables below in merge order.

## Rehydration Kernel (archived predecessor)

| Merged | PR | Classification | Files | Change |
| --- | ---: | --- | ---: | --- |
| `2026-03-07` | [#1](./rehydration-kernel/pr-0001.md) | `feature` | 31 | Add command contract and projection AsyncAPI |
| `2026-03-07` | [#2](./rehydration-kernel/pr-0002.md) | `fix` | 1 | Pass GITHUB_TOKEN to buf setup |
| `2026-03-07` | [#3](./rehydration-kernel/pr-0003.md) | `ci` | 1 | Scope workflow permissions per job |
| `2026-03-07` | [#4](./rehydration-kernel/pr-0004.md) | `feature` | 8 | Introduce structured projection packs |
| `2026-03-07` | [#5](./rehydration-kernel/pr-0005.md) | `feature` | 9 | Implement Neo4j projection read model |
| `2026-03-07` | [#6](./rehydration-kernel/pr-0006.md) | `feature` | 19 | Build node-centric projection consumer foundation |
| `2026-03-08` | [#7](./rehydration-kernel/pr-0007.md) | `refactor` | 17 | Refactor Neo4j and NATS adapters |
| `2026-03-09` | [#8](./rehydration-kernel/pr-0008.md) | `feature` | 49 | Add Context Service compatibility shell |
| `2026-03-09` | [#9](./rehydration-kernel/pr-0009.md) | `feature` | 16 | Implement compatibility ValidateScope parity |
| `2026-03-09` | [#10](./rehydration-kernel/pr-0010.md) | `feature` | 27 | Add Phase 2 read-path golden tests |
| `2026-03-09` | [#11](./rehydration-kernel/pr-0011.md) | `fix` | 31 | Close Phase 2 semantic parity gaps |
| `2026-03-09` | [#12](./rehydration-kernel/pr-0012.md) | `feature` | 32 | Implement async NATS compatibility flows |
| `2026-03-09` | [#13](./rehydration-kernel/pr-0013.md) | `feature` | 29 | Wire compatibility NATS runtime |
| `2026-03-10` | [#14](./rehydration-kernel/pr-0014.md) | `documentation` | 4 | Define swe-ai-fleet integration strategy |
| `2026-03-10` | [#15](./rehydration-kernel/pr-0015.md) | `feature` | 4 | Add swe-ai-fleet integration checklist |
| `2026-03-10` | [#16](./rehydration-kernel/pr-0016.md) | `documentation` | 10 | Freeze kernel integration contract |
| `2026-03-10` | [#17](./rehydration-kernel/pr-0017.md) | `fix` | 6 | Harden kernel contract CI |
| `2026-03-10` | [#18](./rehydration-kernel/pr-0018.md) | `fix` | 1 | Harden AsyncAPI contract tests |
| `2026-03-10` | [#19](./rehydration-kernel/pr-0019.md) | `feature` | 20 | Add kernel contract reference fixtures |
| `2026-03-10` | [#20](./rehydration-kernel/pr-0020.md) | `feature` | 20 | Add agentic integration end-to-end tests |
| `2026-03-10` | [#21](./rehydration-kernel/pr-0021.md) | `feature` | 28 | Add runtime reference client |
| `2026-03-10` | [#22](./rehydration-kernel/pr-0022.md) | `feature` | 18 | Add event-driven runtime trigger e2e |
| `2026-03-11` | [#24](./rehydration-kernel/pr-0024.md) | `feature` | 11 | Add starship rehydration agentic e2e |
| `2026-03-11` | [#23](./rehydration-kernel/pr-0023.md) | `feature` | 47 | Add public distribution packaging |
| `2026-03-11` | [#25](./rehydration-kernel/pr-0025.md) | `refactor` | 1 | Extract Helm HTTPS protocol constant |
| `2026-03-12` | [#27](./rehydration-kernel/pr-0027.md) | `documentation` | 16 | Document LLM response determinism strategy |
| `2026-03-12` | [#28](./rehydration-kernel/pr-0028.md) | `refactor` | 36 | Reclassify Starship scenario as demo |
| `2026-03-12` | [#26](./rehydration-kernel/pr-0026.md) | `feature` | 26 | Add projection runtime state and deployment wiring |
| `2026-03-15` | [#29](./rehydration-kernel/pr-0029.md) | `ci` | 8 | Modernize workflow JavaScript runtime |
| `2026-03-15` | [#30](./rehydration-kernel/pr-0030.md) | `fix` | 1 | Fix publish-distribution metadata action pin |
| `2026-03-17` | [#31](./rehydration-kernel/pr-0031.md) | `fix` | 1 | Fix publish-distribution build-push action pin |
| `2026-03-17` | [#32](./rehydration-kernel/pr-0032.md) | `feature` | 17 | Add gRPC TLS and mTLS transport modes |
| `2026-03-17` | [#33](./rehydration-kernel/pr-0033.md) | `feature` | 6 | Add Helm TLS and mTLS deployment wiring |
| `2026-03-17` | [#34](./rehydration-kernel/pr-0034.md) | `feature` | 18 | Add NATS TLS runtime configuration |
| `2026-03-17` | [#35](./rehydration-kernel/pr-0035.md) | `feature` | 8 | Add Valkey TLS and rediss support |
| `2026-03-17` | [#36](./rehydration-kernel/pr-0036.md) | `feature` | 6 | Add outbound TLS Helm wiring |
| `2026-03-18` | [#38](./rehydration-kernel/pr-0038.md) | `feature` | 7 | Add Kubernetes transport security smoke script |
| `2026-03-18` | [#39](./rehydration-kernel/pr-0039.md) | `maintenance` | 1 | Clean up Kubernetes transport smoke findings |
| `2026-03-18` | [#41](./rehydration-kernel/pr-0041.md) | `feature` | 26 | Add kernel full-journey E2E coverage |
| `2026-03-18` | [#43](./rehydration-kernel/pr-0043.md) | `feature` | 3 | Add graph explorer planning docs |
| `2026-03-19` | [#44](./rehydration-kernel/pr-0044.md) | `feature` | 30 | Add native depth-aware context traversal |
| `2026-03-19` | [#45](./rehydration-kernel/pr-0045.md) | `feature` | 15 | Add graph explorer node detail query |
| `2026-03-19` | [#46](./rehydration-kernel/pr-0046.md) | `test` | 8 | Cover explorer deep journey and node detail E2E |
| `2026-03-19` | [#47](./rehydration-kernel/pr-0047.md) | `fix` | 5 | Close graph explorer documentation gaps |
| `2026-03-22` | [#48](./rehydration-kernel/pr-0048.md) | `feature` | 28 | Add path-based context rehydration endpoint |
| `2026-03-22` | [#49](./rehydration-kernel/pr-0049.md) | `fix` | 2 | Fix GHCR publishing credentials |
| `2026-03-23` | [#50](./rehydration-kernel/pr-0050.md) | `feature` | 158 | feat: add explanatory relation rehydration artifact |
| `2026-03-23` | [#51](./rehydration-kernel/pr-0051.md) | `removal` | 185 | Remove compatibility layer and require kernel infra |
| `2026-03-23` | [#52](./rehydration-kernel/pr-0052.md) | `removal` | 13 | Remove compatibility layer, refresh paper artifact, fix doc gaps |
| `2026-03-23` | [#42](./rehydration-kernel/pr-0042.md) | `feature` | 16 | Add ingress and staged Neo4j TLS Helm profiles |
| `2026-03-23` | [#53](./rehydration-kernel/pr-0053.md) | `fix` | 23 | Phase 0: fix false semantics in v1beta1 contract |
| `2026-03-23` | [#54](./rehydration-kernel/pr-0054.md) | `removal` | 30 | P2: real UpdateContext with event store, remove admin HTTP, security model |
| `2026-03-23` | [#55](./rehydration-kernel/pr-0055.md) | `feature` | 12 | JetStream event store, tiktoken cl100k_base, OpenTelemetry |
| `2026-03-23` | [#56](./rehydration-kernel/pr-0056.md) | `feature` | 26 | Wire NatsContextEventStore in server, improve test coverage |
| `2026-03-24` | [#57](./rehydration-kernel/pr-0057.md) | `removal` | 57 | Honesty pass: fix token bug, remove admin, validate content hash, salience by semantic_class |
| `2026-03-24` | [#58](./rehydration-kernel/pr-0058.md) | `refactor` | 27 | Recalibrate paper with cl100k_base tokenizer |
| `2026-03-24` | [#59](./rehydration-kernel/pr-0059.md) | `removal` | 14 | Remove snapshot semantics from UpdateContext command path |
| `2026-03-24` | [#60](./rehydration-kernel/pr-0060.md) | `feature` | 21 | Fase A SOTA: dataset generator + latency metrics |
| `2026-03-24` | [#61](./rehydration-kernel/pr-0061.md) | `feature` | 16 | Fase B SOTA: vLLM in the loop with LLM-as-judge on dual GPUs |
| `2026-03-25` | [#62](./rehydration-kernel/pr-0062.md) | `feature` | 13 | Frontier model benchmark: GPT-5.4 + Claude Opus 4 |
| `2026-03-25` | [#63](./rehydration-kernel/pr-0063.md) | `feature` | 27 | Bundle multi-resolution: L0 Summary, L1 Causal Spine, L2 Evidence Pack |
| `2026-03-25` | [#64](./rehydration-kernel/pr-0064.md) | `feature` | 30 | RehydrationMode: auto-select resume_focused under token pressure |
| `2026-03-25` | [#37](./rehydration-kernel/pr-0037.md) | `dependency` | 1 | Bump aws-lc-sys from 0.37.0 to 0.37.1 in the cargo group across 1 directory |
| `2026-03-30` | [#65](./rehydration-kernel/pr-0065.md) | `fix` | 2 | Fix full journey render budget |
| `2026-03-30` | [#66](./rehydration-kernel/pr-0066.md) | `maintenance` | 4 | Raise Rust coverage with Sonar exclusions |
| `2026-04-02` | [#68](./rehydration-kernel/pr-0068.md) | `feature` | 9 | feat: E2E helm tests — mTLS, fail-fast, version-aligned |
| `2026-04-02` | [#69](./rehydration-kernel/pr-0069.md) | `fix` | 2 | fix(helm): disambiguate kernel/neo4j label selector |
| `2026-04-04` | [#74](./rehydration-kernel/pr-0074.md) | `documentation` | 1 | docs: production incident resolution — kernel perspective |
| `2026-04-08` | [#75](./rehydration-kernel/pr-0075.md) | `fix` | 130 | [codex] Harden eval TLS and add NATS/Valkey exporters |
| `2026-04-09` | [#76](./rehydration-kernel/pr-0076.md) | `fix` | 9 | [codex] Fix OTLP endpoint wiring and Helm mTLS |
| `2026-04-09` | [#77](./rehydration-kernel/pr-0077.md) | `feature` | 37 | feat(testkit): add repair-judge graph batch flow |
| `2026-04-09` | [#78](./rehydration-kernel/pr-0078.md) | `documentation` | 11 | [codex] Align GraphBatch docs and archive stale notes |
| `2026-04-09` | [#79](./rehydration-kernel/pr-0079.md) | `feature` | 13 | feat(testkit): add PIR kernel roundtrip smoke |
| `2026-04-10` | [#80](./rehydration-kernel/pr-0080.md) | `feature` | 23 | [codex] Add reranker-backed graph batch smokes |
| `2026-04-10` | [#81](./rehydration-kernel/pr-0081.md) | `feature` | 8 | [codex] Add PIR integration plan and live context smokes |
| `2026-04-10` | [#82](./rehydration-kernel/pr-0082.md) | `feature` | 7 | [codex] Add PIR corrective wave smoke and evidence |
| `2026-04-11` | [#83](./rehydration-kernel/pr-0083.md) | `feature` | 33 | [codex] Add blind PIR validation and contract runner |
| `2026-04-12` | [#84](./rehydration-kernel/pr-0084.md) | `documentation` | 13 | Document PIR fix_planning migration and experiments |
| `2026-04-14` | [#85](./rehydration-kernel/pr-0085.md) | `documentation` | 9 | [codex] Document fix planning baseline |
| `2026-05-03` | [#87](./rehydration-kernel/pr-0087.md) | `feature` | 46 | Connect MCP adapter to live kernel over TLS |
| `2026-05-03` | [#88](./rehydration-kernel/pr-0088.md) | `feature` | 37 | feat(mcp): ingest memory through live kernel |
| `2026-05-04` | [#89](./rehydration-kernel/pr-0089.md) | `feature` | 33 | [codex] Add Phase A multidimensional temporal traversal |
| `2026-05-05` | [#90](./rehydration-kernel/pr-0090.md) | `feature` | 120 | Kernel memory service gRPC and MCP traversal |
| `2026-05-07` | [#91](./rehydration-kernel/pr-0091.md) | `documentation` | 114 | [codex] Document Cohere reranker probe |
| `2026-05-08` | [#92](./rehydration-kernel/pr-0092.md) | `documentation` | 4 | [codex] Document MemoryArena 100-task benchmark and operator plan |
| `2026-05-09` | [#93](./rehydration-kernel/pr-0093.md) | `feature` | 32 | Add LongMemEval smart writer and benchmark docs |
| `2026-05-11` | [#94](./rehydration-kernel/pr-0094.md) | `feature` | 20 | Add kernel operator training pipeline |
| `2026-05-11` | [#95](./rehydration-kernel/pr-0095.md) | `test` | 5 | Validate kernel operator explicit holdout |
| `2026-05-11` | [#96](./rehydration-kernel/pr-0096.md) | `feature` | 3 | Add operator prediction de-anonymizer |
| `2026-05-11` | [#97](./rehydration-kernel/pr-0097.md) | `feature` | 3 | [codex] Add live MCP replay for operator predictions |
| `2026-05-11` | [#98](./rehydration-kernel/pr-0098.md) | `documentation` | 3 | [codex] Document next operator scale gate |
| `2026-05-11` | [#99](./rehydration-kernel/pr-0099.md) | `feature` | 14 | [codex] Add testkit TLS parity and operator publication assets |
| `2026-05-12` | [#100](./rehydration-kernel/pr-0100.md) | `feature` | 21 | [codex] Add P1.11 pagination page metadata |
| `2026-05-12` | [#101](./rehydration-kernel/pr-0101.md) | `fix` | 4 | Preserve operator page metadata in trajectories |
| `2026-05-12` | [#102](./rehydration-kernel/pr-0102.md) | `fix` | 8 | Fix operator MCP replay for mixed LongMemEval refs |
| `2026-05-14` | [#103](./rehydration-kernel/pr-0103.md) | `feature` | 68 | [codex] Add operator KMP contract coverage |
| `2026-05-14` | [#104](./rehydration-kernel/pr-0104.md) | `documentation` | 1 | docs(readme): link to underpassai.com homepage |
| `2026-05-16` | [#105](./rehydration-kernel/pr-0105.md) | `feature` | 40 | [codex] Add operator v5 real replay baseline |
| `2026-05-16` | [#106](./rehydration-kernel/pr-0106.md) | `documentation` | 10 | Document Operator P111 scale gate |
| `2026-05-16` | [#107](./rehydration-kernel/pr-0107.md) | `feature` | 6 | Add writer pre-read operator gate |
| `2026-05-16` | [#108](./rehydration-kernel/pr-0108.md) | `documentation` | 2 | Document P111 writer pre-read mixed gate |
| `2026-05-16` | [#109](./rehydration-kernel/pr-0109.md) | `feature` | 8 | Add writer pre-read v2 diversity gate |
| `2026-05-26` | [#111](./rehydration-kernel/pr-0111.md) | `feature` | 5 | Add E2E regeneration preflight tooling |
| `2026-05-26` | [#112](./rehydration-kernel/pr-0112.md) | `feature` | 7 | Add Neo4j projection schema migration |
| `2026-05-29` | [#113](./rehydration-kernel/pr-0113.md) | `documentation` | 7 | docs(operator): anonymization-mandatory + divergence banners |
| `2026-05-30` | [#114](./rehydration-kernel/pr-0114.md) | `documentation` | 2 | docs: clarify Operator (external, benchmark-only) + agent-readable explainer |
| `2026-05-30` | [#115](./rehydration-kernel/pr-0115.md) | `feature` | 8 | feat(kmp): promote per-response context-coverage signals into the MCP/API contract |
| `2026-06-02` | [#116](./rehydration-kernel/pr-0116.md) | `feature` | 10 | feat(kmp): opt-in Wake entry budget for near window-expansion (max_entries + frontier_size) |
| `2026-06-02` | [#117](./rehydration-kernel/pr-0117.md) | `documentation` | 6 | docs(kmp): sync contract + ops docs with current code (desync audit, MEDIO) |
| `2026-06-02` | [#118](./rehydration-kernel/pr-0118.md) | `documentation` | 21 | docs(kmp): sync BAJO desyncs + flag historical records (audit, pass 2) |
| `2026-06-02` | [#120](./rehydration-kernel/pr-0120.md) | `documentation` | 11 | docs: catalog hygiene + legacy roadmap banners + status sync |
| `2026-06-02` | [#121](./rehydration-kernel/pr-0121.md) | `documentation` | 1 | docs: label README benchmark block as a local scorecard |
| `2026-06-02` | [#119](./rehydration-kernel/pr-0119.md) | `documentation` | 6 | docs(kmp): correct authoritative-doc accuracy + fix broken API links |
| `2026-06-02` | [#122](./rehydration-kernel/pr-0122.md) | `documentation` | 8 | docs: low-severity accuracy fixes + draft caveats |
| `2026-06-02` | [#123](./rehydration-kernel/pr-0123.md) | `documentation` | 1 | docs: move Legal to the bottom of the README |
| `2026-06-02` | [#124](./rehydration-kernel/pr-0124.md) | `documentation` | 1 | docs: README — Operator (0.5B) benchmark + KMP distributions |
| `2026-06-02` | [#125](./rehydration-kernel/pr-0125.md) | `removal` | 119 | chore: archive stale run configs + drop vendored async-nats |
| `2026-07-21` | [#126](./rehydration-kernel/pr-0126.md) | `documentation` | 2 | docs: add KMP embedded edition roadmap |
| `2026-07-21` | [#127](./rehydration-kernel/pr-0127.md) | `documentation` | 10 | docs(adr): E0 embedded edition decisions (ADR-009..013) |
| `2026-07-21` | [#129](./rehydration-kernel/pr-0129.md) | `feature` | 20 | feat(conformance): backend-independent KMP conformance suite (E1) |
| `2026-07-21` | [#130](./rehydration-kernel/pr-0130.md) | `feature` | 25 | feat(embedded): rehydration-adapter-embedded — every kernel port on redb (E2) |
| `2026-07-22` | [#131](./rehydration-kernel/pr-0131.md) | `feature` | 34 | feat(mcp): embedded backend — the kernel in-process (E3 core) |
| `2026-07-22` | [#132](./rehydration-kernel/pr-0132.md) | `feature` | 24 | feat(embedded): local quality-telemetry journal (E3, ADR-014) |
| `2026-07-22` | [#133](./rehydration-kernel/pr-0133.md) | `feature` | 7 | feat(mcp): embedded log file + binary gates + Sonar fix for main (E3) |
| `2026-07-23` | [#134](./rehydration-kernel/pr-0134.md) | `feature` | 2 | feat(e4): host integrations — recipes, playbook, two-session demo |
| `2026-07-23` | [#135](./rehydration-kernel/pr-0135.md) | `feature` | 7 | feat(e5): distribution — release matrix, install script, format compatibility |
| `2026-07-23` | [#136](./rehydration-kernel/pr-0136.md) | `feature` | 12 | feat(e6): export/import — the event log as a portable bundle |
| `2026-07-23` | [#137](./rehydration-kernel/pr-0137.md) | `fix` | 2 | fix(e5): cross-compile macOS Intel on arm64 runner; record rich-graph A/B |
| `2026-07-25` | [#138](./rehydration-kernel/pr-0138.md) | `refactor` | 9 | Isolate remote observability from the embedded kernel |
| `2026-07-25` | [#139](./rehydration-kernel/pr-0139.md) | `documentation` | 1 | docs(embedded): sync milestone statuses with what actually shipped |
| `2026-07-25` | [#140](./rehydration-kernel/pr-0140.md) | `ci` | 1 | ci: stop the CodeQL gate from dropping duplicate check runs |
| `2026-07-27` | [#141](./rehydration-kernel/pr-0141.md) | `maintenance` | 10 | chore: move the pinned toolchain to Rust 1.97.1 |
| `2026-07-27` | [#143](./rehydration-kernel/pr-0143.md) | `dependency` | 2 | chore(deps): move the whole OpenTelemetry family to 0.32 |
| `2026-07-27` | [#145](./rehydration-kernel/pr-0145.md) | `ci` | 1 | ci: stop the CodeQL gate from hanging on Dependabot pull requests |
| `2026-07-28` | [#144](./rehydration-kernel/pr-0144.md) | `dependency` | 1 | chore(deps): bump the cargo group across 1 directory with 3 updates |
| `2026-08-01` | [#146](./rehydration-kernel/pr-0146.md) | `fix` | 25 | Fix KMP typed memory round-trips |
| `2026-08-04` | [#147](./rehydration-kernel/pr-0147.md) | `feature` | 15 | feat: publish the consumer memory contract as rehydration-memory-api |
| `2026-08-05` | [#148](./rehydration-kernel/pr-0148.md) | `feature` | 22 | feat: the consumer contract gains a record surface |
| `2026-08-05` | [#149](./rehydration-kernel/pr-0149.md) | `feature` | 6 | feat: the record surface gains relations |
| `2026-08-05` | [#150](./rehydration-kernel/pr-0150.md) | `feature` | 3 | feat: coordinates gain rank, dimensions gain metadata |
| `2026-08-05` | [#151](./rehydration-kernel/pr-0151.md) | `feature` | 4 | feat: a recall names its snapshot and accounts for its rendering |
| `2026-08-05` | [#152](./rehydration-kernel/pr-0152.md) | `feature` | 3 | feat: the kernel accounts for its own recalls; relationships carry their reason |
| `2026-08-11` | [#153](./rehydration-kernel/pr-0153.md) | `feature` | 28 | feat: add embedded memory viewer |

## KMP

| Merged | PR | Classification | Files | Change |
| --- | ---: | --- | ---: | --- |
| `2026-08-14` | [#1](./kmp/pr-0001.md) | `refactor` | 665 | refactor: rename the project to KMP by Underpass |
| `2026-08-14` | [#2](./kmp/pr-0002.md) | `feature` | 16 | feat(plugin): discovery for KMP memory, for the agent and for the human |
| `2026-08-15` | [#3](./kmp/pr-0003.md) | `feature` | 12 | feat(plugin): package the bundle for Codex, Claude Code and Windows |
| `2026-08-15` | [#4](./kmp/pr-0004.md) | `fix` | 9 | fix(deps): close all open Dependabot advisories |
| `2026-08-15` | [#5](./kmp/pr-0005.md) | `feature` | 68 | feat(release): publish the crate chain to crates.io |
| `2026-08-15` | [#6](./kmp/pr-0006.md) | `fix` | 6 | fix(release): publish kmp-application before its dev-dependent, and v0.1.1 |
| `2026-08-15` | [#7](./kmp/pr-0007.md) | `feature` | 15 | feat(embedded): migrate a store this binary refuses to open |
| `2026-08-15` | [#8](./kmp/pr-0008.md) | `release` | 4 | chore: v0.1.2 |
| `2026-08-16` | [#9](./kmp/pr-0009.md) | `documentation` | 7 | docs: lead with the two editions and the real install path |
| `2026-08-16` | [#10](./kmp/pr-0010.md) | `fix` | 6 | fix(plugin): fall back to kmp-mcp on PATH when the bundle has no binary |
| `2026-08-16` | [#11](./kmp/pr-0011.md) | `release` | 7 | chore: v0.1.3 |
| `2026-08-16` | [#12](./kmp/pr-0012.md) | `maintenance` | 6 | chore: point the plugin marketplace at underpass-ai/plugins |
| `2026-08-16` | [#13](./kmp/pr-0013.md) | `fix` | 2 | fix(mcp): kernel_write_memory commits by default |
| `2026-08-16` | [#15](./kmp/pr-0015.md) | `fix` | 1 | fix(plugin): start the MCP server by absolute path |
| `2026-08-16` | [#16](./kmp/pr-0016.md) | `test` | 2 | test(plugin): gate a marketplace-shaped install |
| `2026-08-16` | [#18](./kmp/pr-0018.md) | `refactor` | 14 | refactor(embedded): put a storage seam between the ports and redb |
| `2026-08-16` | [#17](./kmp/pr-0017.md) | `documentation` | 3 | docs(adr): decide what to do about two agent hosts, one memory |
| `2026-08-16` | [#19](./kmp/pr-0019.md) | `feature` | 21 | feat(embedded): SQLite engine behind the seam, opt-in |
| `2026-08-16` | [#20](./kmp/pr-0020.md) | `feature` | 6 | feat(embedded): migrate a redb store into a SQLite one |
| `2026-08-16` | [#21](./kmp/pr-0021.md) | `feature` | 13 | feat(mcp): choose the storage engine from the environment |
| `2026-08-16` | [#22](./kmp/pr-0022.md) | `fix` | 2 | fix(mcp): forward the sqlite feature through kmp-embedded only |
| `2026-08-16` | [#24](./kmp/pr-0024.md) | `feature` | 3 | feat(plugin): make catching up reachable |
| `2026-08-16` | [#23](./kmp/pr-0023.md) | `feature` | 9 | feat(plugin): ship an example memory, and a /kmp:demo that uses it |
| `2026-08-16` | [#26](./kmp/pr-0026.md) | `feature` | 11 | feat(embedded): memory can live in the repository |
| `2026-08-16` | [#27](./kmp/pr-0027.md) | `feature` | 3 | feat(plugin): undo a decision without deleting it |
| `2026-08-16` | [#29](./kmp/pr-0029.md) | `release` | 6 | chore: v0.1.4 |
| `2026-08-16` | [#36](./kmp/pr-0036.md) | `fix` | 5 | fix(embedded): wait out a WAL switch we lost instead of failing the open |
| `2026-08-16` | [#37](./kmp/pr-0037.md) | `fix` | 8 | fix(plugin): let an operator point the launcher at their own binary |
| `2026-08-16` | [#38](./kmp/pr-0038.md) | `release` | 6 | chore: v0.1.5 |
| `2026-08-16` | [#46](./kmp/pr-0046.md) | `fix` | 4 | fix(viewer): make the timeline and Replay ask questions that answer |
| `2026-08-16` | [#47](./kmp/pr-0047.md) | `fix` | 3 | fix(viewer): show a reader a date, and stop showing a snapshot that never resolves |
| `2026-08-17` | [#48](./kmp/pr-0048.md) | `fix` | 4 | fix(viewer): refuse a parameter that is not a number, answer HEAD, and say what the budget budgets |
| `2026-08-17` | [#49](./kmp/pr-0049.md) | `documentation` | 3 | docs: say what dimension scoping is for, not just what it accepts |
| `2026-08-17` | [#50](./kmp/pr-0050.md) | `fix` | 1 | fix(memory): let the first write to a fresh about connect to that about |
| `2026-08-17` | [#51](./kmp/pr-0051.md) | `feature` | 10 | feat(memory): hand wake the cursor it already had |
| `2026-08-17` | [#52](./kmp/pr-0052.md) | `ci` | 2 | ci: skip the gates for a tree that was already proved green |
| `2026-08-17` | [#53](./kmp/pr-0053.md) | `feature` | 7 | feat(memory): mark an entry that a later one replaced |
| `2026-08-17` | [#54](./kmp/pr-0054.md) | `feature` | 5 | feat(embedded): one command to share a memory between two hosts, and a doctor that offers it |
| `2026-08-17` | [#55](./kmp/pr-0055.md) | `test` | 3 | test(kernel): wait for the projection to catch up before asserting its size |
| `2026-08-17` | [#56](./kmp/pr-0056.md) | `feature` | 2 | feat(mcp): leave a trace when the server starts, and when it does not |
| `2026-08-17` | [#58](./kmp/pr-0058.md) | `fix` | 1 | fix(viewer): replay the whole graph, not only what carries a clock |
| `2026-08-17` | [#59](./kmp/pr-0059.md) | `release` | 6 | chore: v0.1.6 |
| `2026-08-17` | [#69](./kmp/pr-0069.md) | `feature` | 35 | Implement KMP issues #61–#68 |
| `2026-08-17` | [#70](./kmp/pr-0070.md) | `release` | 6 | chore: v0.1.7 |
| `2026-08-17` | [#72](./kmp/pr-0072.md) | `fix` | 2 | Fix recall payload truncation under token budgets |
| `2026-08-17` | [#73](./kmp/pr-0073.md) | `release` | 6 | chore: v0.1.8 |
| `2026-08-17` | [#74](./kmp/pr-0074.md) | `ci` | 6 | ci: retry transient Rust toolchain downloads |
| `2026-08-17` | [#75](./kmp/pr-0075.md) | `ci` | 2 | ci: gate relation materialization |
| `2026-08-17` | [#76](./kmp/pr-0076.md) | `test` | 2 | test(kernel): prove out-of-order relation convergence |
| `2026-08-17` | [#77](./kmp/pr-0077.md) | `test` | 2 | test(kernel): prove relation replay idempotency |
| `2026-08-17` | [#78](./kmp/pr-0078.md) | `test` | 2 | test(kernel): keep placeholders out of context |
| `2026-08-17` | [#79](./kmp/pr-0079.md) | `release` | 6 | chore: v0.1.9 |
| `2026-08-17` | [#84](./kmp/pr-0084.md) | `fix` | 1 | Preserve short identifiers during evidence recall |
| `2026-08-17` | [#86](./kmp/pr-0086.md) | `fix` | 1 | Wait for TLS journey projection convergence |
| `2026-08-18` | [#87](./kmp/pr-0087.md) | `fix` | 2 | fix(ask): require strict evidence to cover requested subject |
| `2026-08-18` | [#88](./kmp/pr-0088.md) | `fix` | 4 | fix(ask): scope proof paths and preserve frontier accounting |
| `2026-08-18` | [#89](./kmp/pr-0089.md) | `fix` | 3 | fix(memory): prioritize retained evidence under recall budgets |
| `2026-08-18` | [#93](./kmp/pr-0093.md) | `fix` | 10 | fix(ask): recall supported constraints across paraphrases |
| `2026-08-18` | [#96](./kmp/pr-0096.md) | `fix` | 1 | fix(mcp): budget truncation metadata before fallback |
| `2026-08-18` | [#97](./kmp/pr-0097.md) | `maintenance` | 19 | perf(recall): normalize ask evidence by stable refs |
| `2026-08-18` | [#98](./kmp/pr-0098.md) | `feature` | 14 | feat(mcp): add monotone pageable recall projection |
| `2026-08-18` | [#99](./kmp/pr-0099.md) | `release` | 5 | chore: v0.1.10 |
| `2026-08-18` | [#103](./kmp/pr-0103.md) | `fix` | 1 | fix(ask): preserve graph context during diversification |
| `2026-08-18` | [#104](./kmp/pr-0104.md) | `feature` | 5 | feat: add graph-aware evidence reranker |
| `2026-08-18` | [#105](./kmp/pr-0105.md) | `documentation` | 11 | Teach agents how relation why powers recall and audit |
| `2026-08-18` | [#106](./kmp/pr-0106.md) | `release` | 5 | Release v0.1.11 |
