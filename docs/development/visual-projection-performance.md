# ChronoLoom projection reuse (#779)

An identical `ProjectVisual` request used to load and project the entire selected
temporal source again. The application now retains successful projections in a
service-local LRU. HTTP, embedded MCP and gRPC all call this application operation.

## Identity and lifetime

Every request still opens its operation-wide read snapshot. Reuse requires the
graph port's `GraphReadRevision`, which certifies the graph, bodies and about
index and distinguishes stores and observer incarnations. SQLite's existing
dedicated revision observer catches commits from independent processes and raw
projection writers. An ambiguous snapshot acquisition returns no reuse identity;
adapters without this guarantee keep the uncached path.

The key contains the complete `VisualProjectionQuery`, compared by value: about,
range strings, axis, the whole dimensional selection and selectors, LOD, bins,
page size, cursor and depth. It does not use the public aggregate revision/hash
or a shortened selection digest as a freshness key. Changes to a foreign about
invalidate an all-about result, including the appearance of a new about. The
store-wide revision also conservatively invalidates unaffected abouts and
historical views.

Only successful application results are retained. Caller results are independent
owned copies; no HTTP headers, capability or credential is cached. HTTP checks
authorization on every request and continues returning `Cache-Control: no-store`.
No dynamic ETag or conditional response was added: the existing public aggregate
revision is insufficient to validate every cross-about projection. Node framing
continues to use #539's separate bounded `ReadNodes` snapshot contract.

The LRU retains at most eight queries under a 32 MiB conservative allocation
allowance. Accounting includes query strings and labels, vector capacities,
nested text/clocks and a per-entry B-tree allowance. Large results are returned
without retention. This bounds cache admission, not total process RSS, concurrent
in-flight projections, allocator overhead or caller-owned copies. Inserting a
different revision drops older retained revisions. An older in-flight miss may
finish last and evict newer entries, but it cannot satisfy their different key.
A poisoned cache lock falls back to recomputation; snapshot errors still fail.

## Reproduction

```sh
cargo build --locked -p kmp-adapter-embedded --example visual_projection_benchmark
python3 scripts/performance/visual_projection_cache.py artifacts/performance-779/run
```

The runner refuses to overwrite an evidence directory. It uses synthetic stores
under the repository's disposable `tmp/`, removes them after the run and records
the binary hash, compiler, platform, source hashes, every timing and process RSS.
The baseline is the same application and snapshot provider with the adapter
declining reuse identities. It therefore executes the original complete read and
projection on every call. The cache path advertises the certified revision.

Each fixture checks full result equality before timing. Every timed repeated
result also equals its first result. Fixture assertions verify the selected entry,
label, coordinate and relation counts. Atlas, Episode and Moment are measured
separately on 16, 256 and 2,048 entries, plus a 256-entry large-body fixture.
Two process rounds reverse baseline/cache order, with 20 warm samples each.
First reads and changed-view misses are retained separately. Application timing
excludes setup, equality checks and JSON encoding; encoding has its own samples.
RSS includes the whole process, fixture creation, oracle reads and all LODs.
New-process reads have uncontrolled OS caches and are not OS-cold measurements.
There is no model generation or absolute latency gate.

## Correctness coverage

Native tests compare full cached/uncached results for five clocks, three LODs,
coordinate filters, whole-entry selectors, explicit/all-about scopes and every
page. They cover changed query fields, invalid cursors and ranges, an admitted
continuation after an edit, peer writes, new and edited foreign abouts, historical
views, label/relation removal, lifecycle updates and raw body changes. A
deterministic suspended read finishes after a newer revision has been cached;
both reads still return their own complete snapshot. Unit tests cover LRU and
byte eviction, oversized keys/results, spare capacity and nested clocks.

Real HTTP tests warm the shared service through two independently authorized
viewers, refuse missing/foreign capabilities and observe a peer edit. A real
gRPC test compares all three LODs with direct application output across edits.

Projection cursor semantics are unchanged. The existing cursor can admit some
body-only edits; in that case the cache returns the newly computed page, never
the older retained tail. Projection reuse is not a stronger proof or pagination
consistency contract.
