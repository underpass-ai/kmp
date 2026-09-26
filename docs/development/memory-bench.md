# Memory bench: the private B-real corpus

`scripts/performance/memory_bench` measures KMP on the operator's path: the
release binary, MCP over stdio and disposable stores. Its data contracts live
in [`SCHEMAS.md`](../../scripts/performance/memory_bench/SCHEMAS.md). This page
covers B-real, the corpus built from real `kmp_ask` calls against a frozen copy
of a real store, with gold written by people. B-real is a local release gate.
It never runs in CI and its data never enters the repository.

All commands run from the repository root:

```sh
B='python3 -m scripts.performance.memory_bench'
F=~/Documents/ai/artifacts/kmp-bench-private/2026-09-26
```

## Privacy

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

## Location

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

## Freezing a store

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

## Freezing the rules with a hash

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

## Blind labeling

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

## Agreement and resolution

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

## Running B-real

```sh
$B real run --freeze $F --variant scripts/performance/memory_bench/variants/<name>.toml
$B real run --freeze $F --variant base.toml --variant candidate.toml --cpus 5-9 --samples 3
```

`run` reads only `gold.jsonl`. It runs one or two arms, interleaved ABAB, each
on a fresh copy of `store/`, through `application/run_questions.run_arms`. It
writes the run directories under `<private root>/cache/runs/`, and scoring
reads them from there. The variant's binary must be a path. `git_ref` variants
are built by `quick-a`.

## What is mechanical

These parts are derived by rule and never judged by a person: the transcript
import, the anchors in the words (`anchors-in-words.v1`), the facet template,
the pools, the answerability flags, the question type and the default answer
policy. Everything else in the gold comes from a labeler: shape, facets,
answers, related and wrong subject.
