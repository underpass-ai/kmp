# Recall projection byte sizing performance (#769)

## Problem and boundary

Recall projection fitting must enforce the exact UTF-8 JSON byte ceiling while
preserving stable core fields, page order, continuation actions and progress for
an item larger than the requested budget. The former implementation measured
JSON by allocating a `Vec<u8>` for every whole-response size check. It also
serialized every expansion item again during page fitting and again while
finding the largest item for the progress floor, even though the canonical
projection plan had already serialized each item to bind continuation identity.
The eligible and selected lists cloned every candidate `serde_json::Value` as
well.

This change is internal to `kmp-proto-mapping`. It does not change the recall
contract, byte ceiling, item ordering, detail or entry budgets, cursor identity,
continuation arguments, floor behavior, persisted data or packaging.

## Exact reusable sizing

`ProjectionItem` now records the exact UTF-8 byte length at the same point that
its canonical stable serialization is created. This separate length is needed
because ranked evidence and catalogue labels deliberately replace the stable
key with ordering prefixes. Page admission and the progress-floor maximum reuse
the recorded length without traversing or cloning the item again.

Eligible and selected collections borrow their items from the operation-local
projection plan. Only values admitted to the final response are cloned into the
returned JSON tree. Core fitting records the best character bound during binary
search and constructs that bounded core once after the search instead of cloning
it for every successful midpoint.

Whole-response measurements still have to traverse the complete candidate.
`JsonByteCounter` implements `std::io::Write` and feeds `serde_json::to_writer`,
so those passes count exactly the bytes the existing serializer would emit
without retaining an intermediate output buffer. `used_bytes` is stabilized
from one exact measurement: replacing its decimal integer changes only the
number of decimal digits, so a small numeric fixed point yields the final exact
size. The final budget comparison reuses that value.

There is no cache beyond one projection operation. Each item gains one `usize`
length in the already operation-local plan; borrowed pointer vectors replace
cloned candidate values. The result itself still owns every returned value.

## Correctness controls

The counting writer is checked against `serde_json::to_vec` across nulls,
integers above JavaScript's exact range, empty arrays, quotes, backslashes,
control-character escapes and multi-byte Unicode. A separate fixed-point matrix
checks exact final `used_bytes` for escaped strings and values crossing decimal
digit boundaries. Cached item lengths are independently serialized for both
ranked evidence and label keys.

The native harness follows every `projection.next_action` until completion for
empty, small, medium, shortened-core and oversized-item fixtures. It compares
the exact bytes and SHA-256 of every complete page sequence between the baseline
and candidate, including cursor, page/section accounting, warnings, action
arguments and progress-floor budget changes. The repository's existing byte
budget sweep, typed round trips, detail nesting, cursor binding and shortened
core tests remain the broader contract control.

## Informational controls

The baseline production source is main `59bc1fd4`; harness-only commit
`4fae90aa` adds test counters and the executable without changing release
behavior. The candidate is `3d68a089`. Both use the inherited workspace dev
profile with rustc 1.97.1 on a 20-core aarch64 Cortex-A725 system. Latency
samples run without `LD_PRELOAD`.
Allocation controls use the existing Linux/glibc native counter in separate
processes and subtract one-warmup/zero-additional counts from otherwise
identical one-warmup/25-additional runs. They report cumulative allocation
requests and requested bytes, not live or retained heap. Physical I/O is not
measured and no model or paid API is used.

The serialized quiet run used two processes per revision and shape, each with
three untimed warmups and 30 observations, for 60 warm samples per side. The
exact page bytes and SHA-256 sequence matched for every page of all five
fixtures. That includes 1 empty page, 1 small page, 6 medium pages, 6
shortened-core pages after following its larger restart allowance, and 21
oversized-item pages after following its progress-floor actions.

| shape | former warm p50 / p95 | reusable sizing p50 / p95 | change p50 / p95 |
| --- | ---: | ---: | ---: |
| empty | 0.311 / 0.316 ms | 0.226 / 0.230 ms | -27.3% / -27.2% |
| small | 0.760 / 0.768 ms | 0.462 / 0.470 ms | -39.2% / -38.8% |
| medium | 7.987 / 8.032 ms | 4.983 / 4.996 ms | -37.6% / -37.8% |
| shortened core | 18.746 / 18.877 ms | 16.609 / 16.675 ms | -11.4% / -11.7% |
| oversized item | 14.893 / 14.995 ms | 11.387 / 11.417 ms | -23.5% / -23.9% |

No cold-process latency is reported: fixture construction happens before the
timer, but each timing process deliberately warms the projection three times.
These direct synthetic calls isolate projection fitting and do not represent
end-to-end store or MCP latency.

The test-only pass counter separates traversal from output-buffer allocation.
`full` counts calls that walk a whole candidate or item for byte sizing;
`repeated item` counts the second serialization during page admission. It does
not count the first stable-key serialization, which remains necessary to bind
the cursor.

| shape | former full / repeated item passes | reusable full / repeated item passes | exact returned bytes/items |
| --- | ---: | ---: | ---: |
| small | 21 / 15 | 4 / 0 | 4,002 / 15 |
| medium | 269 / 64 | 4 / 0 | 10,090 / 64 |
| shortened core | 157 / 3 | 20 / 0 | 3,805 / 2 |

The allocation control reports the zero-to-25 additional-projection delta.
Dividing by 25 is descriptive because process allocator state remains part of
the control.

| shape | former requests / requested bytes per projection | reusable requests / requested bytes per projection | total delta |
| --- | ---: | ---: | ---: |
| medium | 11,087 / 1,386,467 B | 5,762 / 668,023 B | -48.0% / -51.8% |
| shortened core | 7,990 / 4,116,196 B | 5,577 / 2,723,832 B | -30.2% / -33.8% |
| oversized item | 2,299 / 1,798,080 B | 1,892 / 830,310 B | -17.7% / -53.8% |

The remaining work is dominated by operations the contract still requires:
building the canonical plan and stable keys, cloning returned JSON values, and
walking every shortened-core midpoint. The change adds one `usize` per planned
item and a pointer vector for eligible and selected items. The controls do not
measure peak RSS or live heap, so they do not establish a retained-memory
improvement.

`artifacts/performance-769` contains the exact baseline source, the complete
measured patch, binary/source/tool hashes, validation record, full page parity,
allocation counts, latency summaries and losslessly compressed raw samples.
