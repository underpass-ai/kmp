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
