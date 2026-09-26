# Memory bench

`scripts/performance/memory_bench` decides roadmap steps with numbers, always
on the operator's path: the release binary, MCP over stdio and disposable
stores. Its data contracts live in
[`SCHEMAS.md`](../../scripts/performance/memory_bench/SCHEMAS.md). This page
covers the working cycle (change, build, `quick-a`, report), the modes and
variants, privacy and licences, and B-real, the private corpus of real asks
with gold written by people.

The bench adds to the existing gates and replaces none of them:
`scripts/ci/retrieval-baseline.sh`, `jev-baseline.sh`, `relate-baseline.sh`, the
per-crate coverage floors, the kmp-mcp architecture gate, the API↔MCP parity and
the proto contract keep running as before.

All commands run from the repository root:

```sh
B='python3 -m scripts.performance.memory_bench'
TT='uv run --no-project --with tiktoken==0.14.0 python -m scripts.performance.memory_bench'
export MEMORY_BENCH_PRIVATE_ROOT=~/Documents/ai/artifacts/kmp-bench-private   # outside the repository
F=$MEMORY_BENCH_PRIVATE_ROOT/2026-09-26
```

`$TT` counts tokens with the pinned tiktoken 0.14.0. Under plain `$B` every
token figure is `null` with its reason; nothing else changes.

## The cycle: change, build, quick-a, report

1. **Change** the code on a branch and commit it. A variant names its binary
   either by path or by `git_ref`; a `git_ref` builds the committed tree only,
   so uncommitted work is invisible to it.
2. **Build.** Two ways, pick one:
   - `cargo build --release -p kmp-mcp`, and a variant with
     `path = "target/release/kmp-mcp"`. Fast when the tree is already built;
     the report records the checkout's commit and whether the tree was dirty.
   - `[binary] git_ref = "<branch or commit>"`. `quick-a` resolves the ref to a
     commit, adds a throwaway git worktree under `tmp/memory-bench/work/`,
     runs `cargo build --release --locked -p kmp-mcp` there with a shared
     `CARGO_TARGET_DIR` (`work/cargo-target`), copies the executable to
     `tmp/memory-bench/bin/<sha256>/kmp-mcp` with `provenance.json` (ref, commit,
     cargo and rustc versions, build seconds, `--version`) and removes the
     worktree. The same commit is a cache hit afterwards. Measured on the GX10:
     139 s for the first build with an empty target directory.
3. **Run quick-a** against the candidate variant:

   ```sh
   $TT prepare                                   # offline check of the pinned tiktoken assets
   $TT quick-a --candidate scripts/performance/memory_bench/variants/examples/parity-template.toml
   ```

   The baseline defaults to `variants/baseline.toml` (this checkout's
   `target/release/kmp-mcp`); `--baseline` names another. `--freeze` picks a
   B-real freeze; by default the latest dated one under the private root is used.
4. **Read the report.** The command prints the summary path and the verdict.
   `summary.md` has the verdict, the arms and their provenance, one row per
   section (baseline → candidate on the headline rates, parity, seconds), the
   judged corpora row by row, and the limitations. Every run section also has
   its full `report.md` (the eleven sections of BENCH_SPEC section 12) beside
   its `report.json`, which is the only source of numbers; `$B render --report
   .../report.json` rebuilds the Markdown from it.

The A/A control of a variant (the same arm against a fresh replica of itself)
is `$TT aa --variant scripts/performance/memory_bench/variants/baseline.toml`.
Every quality delta must be 0, and parity must hold. Tokens are counted twice.
A paged read returns a continuation handle the server mints at random per
process (`read_<32 hex>`), the next request sends it back, and two random
handles tokenize to slightly different counts, so the raw count
(`tokens_journey`) may move by a fraction of a token per journey. The
normalized count (`tokens_journey_normalized`) replaces every handle by a
placeholder of the same length before counting, and that one must not move:
the A/A `holds` only when quality, pages and the normalized tokens are all
exactly 0, and it lists the raw movement apart (`raw_moved`). Measured on
2026-09-26: raw Δ 1.01 tokens per journey on the real store and 0.48 on synth
10^3, normalized Δ 0 on both.

### What quick-a runs

| Section | Corpus | Store | Cache |
|---|---|---|---|
| `real-store` | B-real (`gold.jsonl`, when labelled), hard negatives, identifier guards | frozen copy of the real store, `<freeze>/store/`, a fresh copy per process | private |
| `synth-mono` | synth-v1 seed 7 at 10^3, mono | written once by the baseline, imported by each arm (`build = import`) | public |
| `retrieval-judged` | `retrieval_cases.json`: 35 originals + 18 guarded cases (plain), 35 with narrow rerank from the cassette | one seeded store per case | public |
| `jev-judged` | `jev_cases.json` in cassette replay | one seeded store per case and Jev arm | public |

Parity is not a section of its own: every run section compares the two arms
call by call (`application/parity.py`, after the documented volatile fields)
and a `claim = "parity"` variant is judged by it. B-real is skipped with a
reason until `gold.jsonl` exists. The hard negatives and guards run only when
the rules they were generated under are the rules `config/negatives.sha256`
pins and are registered in the private registry
(`negative_rules.require_registered`), checked before any candidate runs.

Each arm of the real-store and synth sections runs interleaved ABAB in blocks
of 10 questions, one warm-up journey per process, pinned to the latency cores
5-9. The judged corpora and the runs are cached: with the base cached, only the
candidate is measured.

### Time on the GX10

`kmp_ask` costs about 0.7 s on the real store (918 entries, v0.23.0) and 1.3 s
on synth 10^3 mono, where one about holds every entry. That, not the bench, sets
the size of quick-a: its strata are 4 questions per type and form of the hard
negatives (48), 4 per family and polarity of the guards (45) and 4 per synth
type (88), BENCH_SPEC section 11's smoke size: enough to catch a regression
or a broken parity, not to certify a small effect (`full` runs every question).
Measured wall time:

| Run (2026-09-26, GX10, load average about 1) | Measured fresh | Wall |
|---|---|---|
| `quick-a`, v0.23.0 (cached) → `ask-gate` (fresh) | candidate only: real-store 140 s, synth 253 s | **393 s** |
| `quick-a`, v0.23.0 (cached) → `wake-focus` (fresh, Jev replay) | candidate only: real-store 141 s, synth 255 s, judged 16 s | **412 s** |
| `quick-a`, v0.23.0 → `parity-template` (`HEAD`) | both arms of synth (real store and judged cached) | 503 s |
| `aa --variant variants/baseline.toml` | both arms of every section: real-store 278 s, synth 500 s, judged 31 s | 809 s |
| first `quick-a` before the synth stratum (218 synth questions; another kmp-mcp busy on the machine) | both arms of every section | 1423 s |

The example variants end as they should: `parity-template` (`HEAD` of this
branch, BT03 telemetry only) `neutral` with parity 1.0 on every call;
`ask-gate` on a binary without the gate `captura_fallida` (`ask-gate.json: not
read by this kmp-mcp version`); `wake-focus` `no_comparable`, because its
`kmp_curate` paths judgement is not in the cassette it replays.

So quick-a meets its 10-minute budget with the base cached; measuring both
arms from scratch takes about 13.5 minutes, and the summary says when a run
went over budget. In the synth section, 4 `wake_resume` journeys page through
196 wake pages (0.41 s each) and take a third of the section's time.

## Modes

`scripts/performance/memory_bench/config/modes.toml` fixes, per mode, what the
result key of a run does not hold: `max_calls`, `samples`, `repeats`,
`max_bytes` and `timeout_s`, plus the sections, the strata, the synth ladder
and the judged arms. A cached run whose manifest disagrees with its mode is
refused (remove it from the cache to measure it again), so editing a mode never
reuses runs measured under the old parameters. The file's SHA-256 is in every
report (`provenance.rules.modes_sha256`) and summary.

| Mode | Command | What |
|---|---|---|
| `quick-a` | `$TT quick-a --candidate V` | the sections above, smoke strata; budget 600 s |
| `full` | `$TT full --candidate V` | every private question, synth 10^3 and 10^4 on both topologies (10^4 as extra levels of the 10^3 question set), a `max_bytes` sweep {2048, 10000} at 10^3, the judged corpora with the wide arm, and evidence recall on the fetched public benchmarks (`[modes.full.public]`: FactConsolidation 32k, LongMemEval-S, MuSiQue and 2Wiki subsets; a corpus not fetched is skipped with the `fetch` command as its reason; descriptive, it does not vote) |
| `jev` | `$TT jev --candidate V --samples 3 [--record]` | the quick strata with three samples, each with its own Jev fixture |
| `aa` | `$TT aa --variant V` | `quick-a`'s parameters and sections, the candidate a fresh replica of the baseline under a nonce |
| `ci` | `$B ci --variant V` | the A/A of `quick-public` (the judged corpora and synth 10^3 mono, no private section, no network, no key); run by `scripts/ci/memory-bench-quick.sh` in the informative CI job `memory-bench-quick`, which publishes `report.md` and never blocks. `docs/development/bench-floors.tsv` proposes the floors a later gate would hold (floors rise freely, ceilings fall freely; the other way is a reviewed change) |

Three more commands sit beside the modes:

- `$B fetch --dataset NAME [--verify-only]` downloads a public dataset pinned in
  `corpora/datasets.lock.json` and checks its SHA-256 (`application/public_cli.py`;
  `public_cli build|run|plan-full` stay available as a module).
  `public_cli run --corpus factconsolidation --declared` loads the writer-declared
  variant (`factconsolidation-<size>-declared`): the same facts plus the
  `supersedes` a writer following the dataset's own rule ("later facts override
  earlier ones") would declare, each fact over the previous one of its (subject,
  relation). The rule reads the fact list, never the answers. It measures what ask
  does once supersession is declared; the undeclared corpus, where the kernel must
  find the current fact alone, stays the headline.
- `$B mixed --reference PATH [--candidate PATH] [--old PATH]` runs the mixed-version
  and multiprocess scenarios (BENCH_SPEC 10) on a cached 10^3 store and writes
  `tmp/memory-bench/bt18/<time>/report.json` (`kmp.bench.mixed_versions.v1`).
- `$B jev-tail [--self-check]` is the Jev tail test (BENCH_SPEC 9): 2-4 concurrent
  evaluations through a local proxy that injects 429 with Retry-After, delays and
  held connections, reporting the maximum and p99 of the added latency and the
  deadline share per site. Real mode needs `TYPESAFE_API_KEY` in the environment
  and is skipped with its reason without it; `--self-check` measures against a
  loopback stand-in. kmp-mcp pins `https://api.typesafe.ai` and ignores proxies, so
  the client mirrors the binary's retry policy (a test reads it from the Rust
  source) rather than driving the binary through the proxy.

Sweep runs get their own mode name in the result key (`full-mb2048`), because
`max_bytes` is not a key part.

### Verdicts

Every comparison counts questions, not samples. With `--samples 3` the three
samples of a question collapse into one observation per arm before any test
(`application/compare.py`, `collapse`): a rate takes the majority of its samples,
a tie going to the first sample, and a mean takes their mean. McNemar, the paired
bootstrap and the MDE then run over the questions, so 29 questions measured three
times are n = 29, not 87. Each Delta says so (`unit`, `samples`).

Each section's verdict follows the pre-registered rules of
`application/verdict.py` (BENCH_SPEC section 12). The mode's verdict takes the
first value, in the order `verdict_precedence` of `modes.toml` gives (worst
first: `captura_fallida`, `no_comparable`, `regresion`, `indecidible`,
`mejora_con_coste`, `solo_coste`, `mejora`, `neutral`), that any voting section
reached. A section votes when it measured a target of the claim, or when it
failed, was not comparable or regressed (a broken guard anywhere is a
regression). A section whose questions never exercise the target (no wake
question for a wake claim) says `indecidible` by construction and does not
vote. So a candidate earns `mejora` only when every section that measured its
targets says so. The judged corpora
vote only on a regression: a row of the recorded baseline TSV
(`retrieval-baseline.tsv`, `jev-baseline.tsv`; floors may rise, `false_*`
ceilings may fall, counts are exact) that the baseline holds and the candidate
breaks.

## Variants

A variant is one TOML in `scripts/performance/memory_bench/variants/`
(`kmp.bench.variant.v1`, SCHEMAS.md section 4). It pre-registers its claim
(`parity`, `quality` or `cost`), its targets with the smallest effect worth
claiming, its guards and its `n`, and declares its binary, store files,
environment (allowlisted) and Jev mode.

| File | What |
|---|---|
| `variants/baseline.toml` | this checkout's `target/release/kmp-mcp`, no store file (so with the ask gate), no Jev |
| `variants/examples/parity-template.toml` | declared parity of a `git_ref` (default `HEAD`): every call byte-identical after the volatile fields |
| `variants/examples/ask-gate.toml` | a quality claim configured by `ask-gate.json` beside the store (the P4 gate; the default since 26 Sept 2026, so on a current binary it only writes the default out) |
| `variants/examples/ask-gate-off.toml` | the same binary with `ask-gate.json` `{"mode":"off"}`: how to measure without the gate now that it is the default |
| `variants/examples/lifecycle-successor-core.toml` | the P7 lifecycle variant, `ask-gate.json` `{"successor_core":true}`: a standing successor may be cited for the anchor its replaced predecessor named (off by default) |
| `variants/examples/wake-focus.toml` | a Jev arm in replay: `typesafe.json` + `wake-focus.json` and a cassette |

**Store files must be acknowledged.** Each file in `[store_files]` is written
beside the store before the binary starts, and the binary must log a
`kmp_store_config` line with `status = "loaded"` and the same SHA-256
(SCHEMAS.md, Telemetry). Otherwise the run records `VARIANT_NOT_APPLIED` and the
verdict is `captura_fallida`: a variant the binary ignored never becomes a
comparison of two identical binaries. `ask-gate.toml` on a binary without the
gate shows exactly this. A binary older than that telemetry (v0.23.0) cannot
acknowledge any file, so a variant with store files needs a newer binary. The
judged corpora's own fixture files (the rerank and Jev arms) are the exception:
on such a binary their stores are counted as `unverified`, a limitation of the
report, and the corpus's own behavioural checks still apply (a rerank that did
not run, a judgement missing from the cassette, fail the corpus).

**Jev arms.** A variant with `jev = "replay"` gets, per sample, its own Jev
fixture under `jev/<key>/sample-<i>/` of the cache (`runtime/jev_fixture.py`),
keyed by binary, judged configuration, store and question set: a replay copies
the recorded cassette (or, with no recording, the variant's named cassette) and
blocks the network; a judgement that the fixture lacks makes the run
`no_comparable`, never a guess. A binary without judgement telemetry cannot
prove the replay complete: it is `unverified`, a declared limitation, not
`no_comparable`. `jev = "record"` runs only in the `jev` mode with `--record`
and `TYPESAFE_API_KEY` exported; the key is handed to the child as a secret and
never recorded. Quick modes always replay.

## Caches

```text
tmp/memory-bench/                    public cache (git-ignored)
  worlds/ stores/ runs/ reports/     SCHEMAS.md section 7
  reports/<key>/judged.json          judged corpus rows of one arm (kmp.bench.judged.v1)
  reports/<key>/summary.{json,md}    a mode summary with no private section (kmp.bench.mode_summary.v1)
  bin/<sha256>/                      git_ref builds with provenance.json
  jev/<key>/sample-<i>/              Jev fixtures
  work/                              disposable: running stores, build worktrees, cargo-target
$MEMORY_BENCH_PRIVATE_ROOT/cache/    the same tree for private runs, reports and summaries
```

Everything is keyed by content, and nothing is overwritten except a report or
summary rebuilt from the same inputs. `$B cache ls` lists the stores and `$B
cache gc` removes abandoned staging; deleting `work/` at any time costs a
rebuild, nothing else.

## Privacy and licences

- **Private material never enters the repository**, `tmp/` included: the real
  store copy, the real questions, their gold and every run over them live under
  `$MEMORY_BENCH_PRIVATE_ROOT` (on the GX10,
  `~/Documents/ai/artifacts/kmp-bench-private`), which must be outside the
  repository. Writers call `jsonl.guard_private_path`; `private_layout` refuses a
  root inside the repository.
- **Reports hold aggregates only.** A private corpus has no `per_question` rows
  and its parity differences keep counts and JSON paths, never values. A report
  or summary over a private corpus is written to the private cache all the same,
  because it names private digests.
- **The live store is never opened.** Runs open disposable copies of a frozen
  copy; the child environment is token_harness' isolation (its own `HOME`, XDG
  directories and data directory), so the machine's lexical bridge table and
  configuration under `~/.local/share/kmp` and `~/.config/kmp` are not read
  either. A run uses a lexical bridge only when the variant names one
  (`KMP_LEXICAL_BRIDGE`); the judged corpora use the repository's judged table.
- **No network.** Quick modes replay Jev from cassettes; nothing is downloaded.
  `$TT prepare` only verifies the tiktoken assets already on disk.
- **The Jev book and cassettes** keep request hashes and answers, not stored
  text.
- **Licences** (BENCH_SPEC section 14). The synth-v1 worlds and the judged
  corpora are the repository's own. The public benchmarks follow these rules
  (`corpora/datasets.lock.json` records each licence): MuSiQue (CC BY 4.0, ids and gold may be kept with attribution),
  2Wiki (Apache-2.0), LongMemEval and MemoryAgentBench (MIT) may be cached;
  HotpotQA (CC BY-SA 4.0) is evaluation only; LoCoMo (CC BY-NC 4.0) is local
  evaluation only and nothing of it enters the repository (it is a private corpus
  in `domain/question.py`); osunlp/HippoRAG_v2 declares no licence and is never a
  source. tiktoken's assets are fetched once by `token_harness prepare` and
  checked by SHA-256.

## B-real, the private corpus

B-real is the corpus built from real `kmp_ask` calls against a frozen copy of a
real store, with gold written by people. It is a local release gate: it never
runs in CI and its data never enters the repository.

### Privacy

B-real handles three kinds of private material: the questions people asked,
the texts of a real store, and the judgments made about those texts. The rules:

- **Nothing private enters the repository.** That includes the git-ignored
  `tmp/`. Every B-real writer goes through `FreezeDir.path`, which calls
  `domain/jsonl.guard_private_path` and refuses any destination inside the
  repository or outside the freeze directory. `write_questions` refuses a
  private question file inside the repository. The private run cache comes
  from `runtime/layout.private_layout`, which refuses a root inside the
  repository. `tests/test_real_private.py` (class `Privacy`) checks every
  writer, the CLI and a symlink pointing back into the repository.
- **Reports are aggregates only.** `agreement.json` holds counts, κ values and
  the gate status. It names no question, ref or text. Question ids that need
  adjudication go to `labels/disagreements.json`, a private file. The bench
  report (`kmp.bench.report.v1`) carries B-real only as aggregates, with no
  `per_question` rows.
- **The live store is never opened.** The freeze adopts a copy. `store_snapshot`
  and `FreezeDir` refuse paths under `~/.local/share/kmp` and `~/.config/kmp`.
  Pools and runs open disposable copies of the frozen store, never the frozen
  files.
- **The terminal shows no private text** in the non-interactive commands
  (`freeze`, `import`, `rules`, `pool`, `status`, `validate`, `agreement`,
  `resolve`, `run`). Only `label` and `search` show store texts, because the
  labeler needs them to judge.

### Location

The private root has no default. On the GX10 it is
`~/Documents/ai/artifacts/kmp-bench-private`. It holds the rules registry
`rules-registry.jsonl`, which the hard-negative rules share, the private run
cache `cache/`, and one directory per freeze date:

```text
<private root>/<YYYY-MM-DD>/
  store/                 complete KMP data directory: FORMAT_VERSION + store/kernel.sqlite3
  bundle.jsonl           kmp-mcp export of that store (content_digest; rebuilds for other binaries)
  reference/kmp-mcp      pinned reference binary that builds the kernel part of each pool
  freeze.json            kmp.bench.breal.freeze.v1: hashes, integrity check, content digest, reference
  questions.jsonl        kmp.bench.breal.intake.v1: the real asks, anchors and facet templates
  labeling-rules.json    kmp.bench.breal.rules.v1: hashed and registered before the first pool
  pools/<id>.json        kmp.bench.breal.pool.v1: the blind, shuffled pool of each question
  pools/_provenance/     which source put each ref in the pool (do not open while labeling)
  labels/<labeler>/<id>.json    kmp.bench.breal.label.v1, one per question and labeler
  labels/adjudicated/<id>.json  Tirso's resolution of a disagreement
  labels/disagreements.json     ids of the questions that need adjudication
  agreement.json         kmp.bench.breal.agreement.v1 (aggregates)
  gold.jsonl             kmp.bench.question.v1, corpus b-real: the only file a runner reads
  negatives/             hard negatives of this store (BT08)
```

### Freezing a store

1. Stop every writer of the live store. Copy its data directory, which holds
   `FORMAT_VERSION` and `store/`, with `cp -a` into `<date>/store/`.
2. Adopt the copy. `freeze` checks the layout and that the `-wal` file is
   empty, then runs `PRAGMA integrity_check` on an immutable read-only open.
   It copies the reference binary to `reference/kmp-mcp` and exports
   `bundle.jsonl` through that binary on a disposable copy. Finally it writes
   `freeze.json` with the SHA-256 of the store, bundle and binary, KMP's
   `content_digest` and the snapshot digest the hard negatives use.

   ```sh
   $B real freeze --freeze $F --binary /path/to/release/kmp-mcp --source-note "cp -a on <date>, writers stopped"
   ```

3. Import the real asks from the local transcripts:

   ```sh
   $B real import --freeze $F
   ```

   The rule is the one that produced the first 33 asks. It reads Claude Code
   `mcp__plugin_kmp_memory__kmp_ask` calls and Codex `kmp` `kmp_ask` calls from
   2026-09-15 on. It skips paging calls (a continuation or cursor) and drops
   host keys (`context_id`, `purpose`, `page`, `review_token`). An identical
   call counts once, at its first time. Ids are content hashes, so running the
   import again keeps every existing record and appends only new asks. A call
   the server refused for lack of an `about` stays in the file as
   `not_labelable`, with `duplicate_of` naming its retry.

`verify` recomputes the hashes of the store and the reference binary before
every pool, label check and run. A changed file stops the command.

### Freezing the rules with a hash

Pre-registration (BENCH_SPEC section 2) means that the rules which build the
pools and decide the gold are hashed and recorded before any pool exists.

```sh
$B real rules --freeze $F
```

This writes `labeling-rules.json` and appends
`{registered_at, rules_sha256, rules_version}` to
`<private root>/rules-registry.jsonl`. The rules file records:

- the pool: seed 7, the reference binary's kernel top 30 under `best_effort`,
  10 random entries of the about, and every entry that contains an anchor;
- the agreement gate: κ ≥ 0.6 on at least 30 doubly labeled questions;
- the mechanical intake rules (anchors, facet template, question type, default
  answer policy);
- the SHA-256 of the reference binary, the store and `gold_schema.json`, and, for the record, of the
  code that applies the rules (recorded, not enforced).

Afterwards:

- `pool`, `label`, `validate`, `agreement` and `resolve` refuse to run when the
  file's hash is not in the registry, when the file pins another binary or
  store, or when `gold_schema.json` changed.
- Running `rules` again with the same content does nothing. Different content
  is refused. Changing a rule means a new dated freeze, whose pools and labels
  start from zero.
- Every label records the `rules_sha256` and pool digest it was made under. A
  label made under other rules or on another pool does not validate.

Any other rules file, such as the P4 gate rules, is frozen the same way:

```sh
$B real register --freeze $F --file path/to/gate-rules.toml --version p4-gate.v1
```

**Out of sample.** Once the P4 gate rules are registered, the asks made after
their first registration form the validation set:

```sh
$B real import --freeze $F --validation-after p4-gate.v1
```

The resolved questions carry `split:development` or `split:validation`, and
`real run --split validation` runs only the second group. Records already in
`questions.jsonl` keep the split they had.

### Blind labeling

```sh
$B real pool  --freeze $F                                  # every labelable question, deterministic
$B real label --freeze $F --labeler ana --question breal-<id>
```

A labeler sees the question in the agent's words and, when present, in the
user's own words (`asked_as`). They also see the proposed facets and the pool,
shuffled with a seed, with each entry's kind and text. They never see the
kernel's order, which source put an entry in the pool, or what the kernel
answered. They may search the whole about at any time, and entries found that
way can be marked like pool entries.

Commands inside `label` (type `help`):

| Command | Effect |
|---|---|
| `show`, `entry N` | the question and pool; the full texts of one entry |
| `search WORDS` or `search "PHRASE"` | free search in the about, ignoring case and accents; hits are `f1`, `f2`… |
| `facet add NAME WORDS…`, `facet rm`, `facet words`, `facet rename` | the facets the question asks for, in the reader's words |
| `answers FACET N…` | these entries answer the facet |
| `related FACET N…` | same subject, but they do not answer the facet |
| `wrong N…` | entries about another subject |
| `clear N…` | remove every mark |
| `shape singular\|enumerative` | the question's form; required before saving |
| `reason R`, `note TEXT` | optional expected UNKNOWN reason; a free note |
| `save`, `quit` | validate and write `labels/<labeler>/<id>.json`; leave |

Judging rules:

- Judge whether an entry **answers** the facet, not whether it contains the
  anchor. An entry that names `#188` without saying anything about the facet
  is `related`, not `answers`.
- A facet is answerable in the store when it has at least one answer. The
  question is KNOWN when some facet is. Both are derived from the marks.
- The facet template is a proposal: split words taken from the question
  (`facet-split.v1`). Rename, merge, add or remove facets until they match what
  the reader asked for.
- `--script FILE` reads the same commands from a file, for scripted
  corrections and tests.

A label passes `labeling/gold_schema.json`: its `gold` is exactly the
`kmp.bench.question.v1` gold object, restricted to shape, answerable, facets
and `wrong_subject`. It must also pass the invariants in `domain/gold.py`, and
every ref it names must be an entry of the question's about in the frozen
store. `real validate` checks every label file.

### Agreement and resolution

```sh
$B real agreement --freeze $F --labelers ana,ben
$B real resolve   --freeze $F
```

Cohen's κ is computed per facet judgment. Each (question, facet, entry) pair is
one unit with the category `answers`, `related` or `none`. The entries are the
pool plus any ref either labeler found by searching. Facets are aligned by
name. A facet that only one labeler listed counts as `none` on the other side.
The report also gives κ for `answerable_facet`, `shape`, `answerable` and
`wrong_subject`, and the distribution of κ facet by facet. The gate is `pass`
only with at least 30 doubly labeled questions and a facet κ of 0.6 or more.
Below 30 it is `insufficient_n`, and the command exits non-zero whenever the
gate does not pass.

Tirso resolves disagreements. He labels the question as the labeler
`adjudicated`:

```sh
$B real label --freeze $F --labeler adjudicated --question <id from labels/disagreements.json>
```

`resolve` writes `gold.jsonl`. For each question, an adjudicated label wins.
Otherwise it takes labels that agree exactly, or the label of the only labeler.
A question whose labels disagree and has no adjudication stays out of
`gold.jsonl`. The question type is mechanical (`breal-type.v1`):

| Anchors in the words | Gold shape | Type |
|---|---|---|
| yes | enumerative | `enumerative_anchored` |
| yes | singular | `singular_anchored` |
| none | any | `lookup_exact` |

Each question keeps its asked answer policy, or `evidence_or_unknown`, which is
`kmp_ask`'s own default, and its budget. Its tags record the split and how the
gold was resolved (`gold:agreed`, `gold:adjudicated`, `gold:single`).

### Running B-real

```sh
$B real run --freeze $F --variant scripts/performance/memory_bench/variants/<name>.toml
$B real run --freeze $F --variant base.toml --variant candidate.toml --cpus 5-9 --samples 3
```

`run` reads only `gold.jsonl`. It runs one or two arms, interleaved ABAB, each
on a fresh copy of `store/`, through `application/run_questions.run_arms`. It
writes the run directories under `<private root>/cache/runs/`, and scoring
reads them from there. The variant's binary must be a path. `git_ref` variants
are built by `quick-a`.

### What is mechanical

These parts are derived by rule and never judged by a person: the transcript
import, the anchors in the words (`anchors-in-words.v1`), the facet template,
the pools, the answerability flags, the question type and the default answer
policy. Everything else in the gold comes from a labeler: shape, facets,
answers, related and wrong subject.
