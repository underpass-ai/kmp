# Joint evidence materialization

Issue #539 extends the two existing Trace search modes with optional
`search.proof:true`. It reuses the delivered selection algorithms and one
embedded ReadTx, then fetches the selected entries and their declared typed
sources jointly. No new tool, writer field or compression is introduced.

Destination mode materializes selected material candidates, or all returned
routes when no material policy was requested. Seed mode materializes the union
of candidates used by returned groups; candidates outside every group do not
silently become accepted proof. The original route/group indexes and contextual
review state remain authoritative.

The domain snapshot port supplies batch bodies from the same transaction as
node/adjacency reads. Both solvers retain their admission object and N/E budget
through materialization; foreign objects and later attachments cannot bypass
that admission. Sources are deduplicated by ref, while physical source-to-entry
associations and clocks stay explicit. Body existence, canonical text, hash and
revision are independent of structural route availability. Missing/wrong-kind
sources, absent bodies, unknown attachment clocks and work cuts retain gaps.
No semantic truth or historical body-version archive is inferred.

MCP and gRPC use the same typed request/result mappings. Positional page order
is trace, candidates/groups for seek, then objects/supports/gaps. The selection
fingerprint is computed before slicing, including all proof content and sorted
metadata. MCP rejects an old cursor when a hidden source changes, including
when MCP uses the gRPC backend. Direct gRPC retains its positional page cursor;
its client must compare `selection_fingerprint` across pages and restart when
it changes. No transaction spans separate calls.
MCP byte projection keeps whole items and supplies a sufficient next action if
an item cannot fit. `body_bytes` measures canonical loaded text; the response
byte ceiling does not impose a storage allocation bound.

Core, real gRPC and native paired controls must establish proof equivalence
before performance claims. The comparator is a quiescent base Trace plus one
complete Inspect per distinct selected entry. Existing operation snapshots and
Inspect batching are already in 0.17 and are not new gains. Record all calls,
pages, bytes, failed attempts, smaller-case regressions, profile and process
startup separately. Report unmeasured phases or allocations as unmeasured.

## Acceptance record for #539

The implementation predates this record. The September 15, 2026 acceptance
run adds a reproducible oracle and two residual regressions; it does not claim
the previously delivered snapshot, batching or viewer work as a new change.

| Contract | Reproducible control |
| --- | --- |
| One snapshot for selected graph, entries, source arrows and bodies during a peer write | `trace_proof_snapshot_tests::{selected_paths_sources_and_bodies_remain_one_state_after_independent_commit, independent_writer_between_selected_graph_and_bodies_cannot_mix_either_mode}` and real gRPC `embedded_snapshot::grpc_inspect_and_trace_proof_keep_graph_body_and_sources_together_during_peer_writes` |
| Exact body, source metadata, coordinates, hashes, revisions, support clocks, `why`, evidence, ids and order | `proof_batch_acceptance.py` compares each reconstructed joint result with Trace plus rich Inspect calls over copies of the same closed store; all four shapes passed |
| One joint body-port operation, with the same domain value as serial reads | `joint_trace_proof::one_joint_body_operation_is_exactly_equivalent_to_the_serial_body_oracle` covers destination and seek; the count is calls to the simulated `TraceSnapshotReader` port, not SQL statements or allocations |
| Node, edge and canonical body/response byte bounds retain whole typed items and explicit gaps | `joint_trace_proof::{missing_sources_at_capacity_are_not_confused_with_unread_sources, exact_node_capacity_remains_partial_without_probing_unreserved_endpoints}`, `bounded_body_delivery`, and `reader_cards` expansion controls |
| Scope and time do not certify foreign, missing, unknown-clock, expired or superseded material | `trace_proof::{foreign_and_ownerless_sources_never_expose_payloads_or_certify_proof, source_attachment_cut_and_unknown_clocks_qualify_group_completeness, validity_cut_excludes_expired_support_and_preserves_canonical_supersession_status}` |
| Interrupted pages reconstruct the original selection; changes outside the visible page invalidate the cursor | the acceptance runner follows every returned action for 64-entry shapes; `reader_cards::{following_every_offered_action_recovers_the_whole_selection_across_pages, changing_a_source_body_outside_the_visible_page_invalidates_the_proof_cursor}` |
| Embedded and gRPC projections preserve the same typed content | real gRPC `evidence_seek::joined_endpoints_share_sources_and_keep_all_proof_pages`, `read_nodes_parity::scoped_node_batches_match_embedded_over_real_grpc_including_budgets`, and MCP/kernel semantic parity |

The oracle is deliberately the rich audit path: one base Trace followed by
`Inspect(details,incoming,outgoing,raw)` for every selected entry. It is not a
minimum-call client. The only qualified presentation difference is Inspect's
`evidence[].metadata.proof_role=stored_evidence`; the joint Trace carries the
same role as the typed source object's `kind=memory_evidence`. The runner
requires that exact pair, then compares all persisted and semantic fields
without normalization.

The measured fixture and frozen-binary provenance are in
`artifacts/proof-batch-acceptance-20260915`. Each path has 20 warm samples,
three untimed warmups and one first read in a fresh process over its own copy of
the same quiescent store. The 64-entry joint results intentionally span seven
or eight MCP pages. Every slow sample from the successful run remains in the
compressed JSONL traces.
The 8 MB budget is a per-response allowance; the oracle receives it once for
Trace and once for every Inspect, while the joint path receives it on each
page. It is not a matched total-context budget between the two paths.

| Shape | Selected sources | RPCs oracle → joint | First-process sum RPC ms oracle → joint | Warm p50 ms oracle → joint | Warm p95 ms oracle → joint | Response bytes oracle → joint |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 distinct | 1 | 2 → 1 | 13.65 → 23.68 | 4.01 → 2.80 | 5.59 → 2.97 | 5,406 → 4,363 |
| 8 distinct | 8 | 9 → 1 | 35.06 → 19.89 | 40.76 → 12.45 | 48.69 → 13.51 | 41,066 → 23,721 |
| 64 distinct | 64 | 65 → 8 | 297.37 → 219.32 | 345.81 → 214.25 | 377.87 → 293.43 | 326,719 → 199,306 |
| 64 shared | 8 | 65 → 7 | 346.24 → 255.29 | 363.50 → 209.33 | 407.01 → 215.07 | 389,961 → 165,328 |

“ms” is the sum of client wait time for the RPCs in one operation. It excludes
JSON parsing, gzip trace writes, client work between calls and process startup.
The first-process result does not evict the operating-system cache; the
one-entry joint first read is slower and is retained above. Peak process VmHWM
is recorded, but it is neither an allocation count nor attributable solely to
the query. Physical I/O and allocations were not measured. These results apply
to this frozen fixture, binary and rich audit comparator; they do not establish
a universal ratio for all clients.

The timed capture used runner SHA-256
`aa7b263a71067af0132e2b81c059defc6686e695b6b9184be6e96cc36bbb5555`.
That version followed the exact returned action and checked reconstructed
semantic content, but did not assert page arithmetic. After capture, the
current runner added those assertions. The separate
`proof_batch_capture_audit.py` checked all 96 recorded joint operations for
contiguous offsets, stable totals, exact section counts, final completion,
cursor/action agreement, unique and complete object sets, and empty gaps. Its
own hash and every evaluated capture hash are recorded in
`post-capture-pagination-audit.json`; all captures passed. This post-capture
check validates the preserved responses and does not retroactively change the
timing runner's source hash.

### Failed-attempt preservation gap

Four earlier failed-run directories were mistakenly deleted as scratch at
2026-09-14T22:49:47.690Z, after the successful run had been recorded. This
removed 38 untracked files: five from the projection-normalization attempt,
five from the MCP fingerprint attempt, and fourteen each from the pagination
and page-warning attempts. Their original environment files, fixtures, partial
summaries and gzip JSON-RPC traces are irrecoverable. They were never Git
objects, and no workspace copy remains.

`failed-attempt-ledger.json` records this integrity failure, the exact deleted
file inventory, the runner commit and independently computed runner hash for
each attempt, exact command-output hashes and the recovered failure. For the
pagination and page-warning attempts, the task execution log also retained the
complete printed summaries for the successful 1- and 8-entry shapes; the
ledger preserves those values. It leaves unavailable timings, counts and file
hashes unknown. No fixture or trace was rerun or regenerated under an old name.
The deletion did not touch the separate successful-run directory or its 96
operation post-capture audit.

Reproduce after freezing a development-profile `kmp-mcp` outside Cargo
`target/`:

```text
python3 scripts/performance/proof_batch_acceptance.py \
  /path/to/frozen/kmp-mcp artifacts/proof-batch-acceptance-YYYYMMDD \
  tmp/proof-batch-run 20
```

The runner requires a new output directory, creates one seed store per shape,
copies it only after the writer closes, records source/runner/binary hashes and
host/profile details, and removes its scratch stores on exit. No model call is
part of seeding, comparison or measurement.

### Physical backend and complete-operation follow-up

An experimental patch series applied to the exact clean main commit
`fc352d7d37ac0480cc79120a6288bfd446fa4c86` records real SQLite `trace_v2` events,
VM steps and returned rows, logical table reads, body batch slots and inclusive
application, adapter, projection, mapping and encoding phases for each serial
RPC when `EVAL539_PROFILE=1`. The patches and their hashes live in
`instrumentation-patches/`; its manifest records clean application, diffstat,
scope and the enabled real-SQLite control command. None of this instrumentation
is present in the product source tree or compiled default binary delivered by
this PR.

`artifacts/proof-batch-physical-20260915` repeats the four exact-equivalence
fixtures with ten measured operations, three warmups and one process-first
operation per path. The runner reports preparation, process startup plus
initialize, summed RPC wait and complete client wall time separately. Both
paths have the same explicit 8,000,000-byte **total traversal** allowance and
the run rejects an operation whose cumulative responses exceed it. Each MCP
call still declares the same value as its per-response API ceiling; observed
complete traversals are far below the shared total (largest: 390,034 oracle
bytes and 199,322 joint bytes).
Each fixture arm uses its own temporary store and dedicated process, disables
the viewer, writes stderr to a dedicated file and issues one stdin request at a
time. Initialize, seeding and warmups remain identifiable and excluded from the
ten measured operations. The post-capture auditor requires a bijection between
request and profile ids/method/tools, zero nesting and VM-delta errors, one
SQLite profile per statement, and the required RPC/backend/schema/encoding
phase paths.

| Shape | Warm client p50 ms oracle → joint | RPCs | SQLite statements p50 per complete operation oracle → joint | VM steps p50 oracle → joint |
| --- | ---: | ---: | ---: | ---: |
| 1 distinct | 7.66 → 3.90 | 2 → 1 | 26 → 11 | 277 → 135 |
| 8 distinct | 51.57 → 12.64 | 9 → 1 | 222 → 81 | 2,785 → 1,177 |
| 64 distinct | 471.09 → 251.53 | 65 → 8 | 1,790 → 5,128 | 22,945 → 76,168 |
| 64 shared | 538.66 → 247.66 | 65 → 7 | 2,686 → 3,703 | 38,121 → 58,807 |

The high-degree joint path reduces round trips, response bytes and observed
client latency while executing more SQLite statements and VM steps because
each continuation reconstructs the bounded selection in a new transaction.
This retained regression is a concrete limit of the current pagination model;
larger pages may reduce it but would change response shape and are not inferred
as a fix here. SQLite profile duration is statement execution reported by the
observer, not physical disk I/O. `VmHWM` is still process-wide, and a fresh
process still does not imply an empty operating-system cache.
All elapsed and inclusive phase timings in this table come from the patched
instrumented binary; no enabled-versus-disabled calibration was run, so they
must not be presented as production latency. SQLite statements, rows and VM
steps are the physical backend evidence. The exact semantic response equality,
RPC counts and response bytes are independently checked by the runner.

`proof_batch_physical_audit.py` reconstructs complete operations from the
preserved JSON-RPC traces, joins each request id to its physical profile and
writes `physical-summary.json`, including capture and auditor hashes. Raw
physical profiles are preserved losslessly as `*-physical.json.gz`;
`compression-manifest.json` records each original and compressed hash, and the
decompressed bytes were verified against the originals from commit `bd7681a8`.
The
previous 38 lost failed-attempt files remain lost and are only described by
`failed-attempt-ledger.json`; this follow-up does not recreate them.

For a new physical run, use
`proof_batch_physical_run.py REPOSITORY OUTPUT SCRATCH SAMPLES`. The wrapper
refuses a dirty repository, verifies that the frozen base exists and remains an
ancestor of `origin/main`, verifies patch hashes,
creates a detached temporary worktree, applies every patch, executes the
enabled prepared-statement/partial-row SQLite control, builds and freezes that
isolated binary, runs the serial acceptance capture under `OUTPUT/capture`, audits it, records patch
validation and removes the temporary worktree and stores.
