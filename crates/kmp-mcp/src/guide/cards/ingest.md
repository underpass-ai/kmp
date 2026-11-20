`kmp_ingest` writes one canonical memory document: entries, relations and
evidence exactly as you declare them, validated, with no semantic planning.
Prefer `kmp_write_memory` for facts from a source; use ingest to restore or
import a document you already hold in canonical form.

Send `about`, a stable `idempotency_key`, `provenance` (`source_kind`,
`source_agent`, `observed_at`) and `memory` with its `dimensions` and
`entries` (each `id`, `kind`, `text`, `coordinates`), plus optional
`relations` (`from`, `to`, `rel`, `class`) and `evidence`. Every id stays
inside the about. `dry_run:true` validates without writing. `label_policy`
decides what a resembling new label does; `refuse` stops the write.
`default_observation_to_ingestion` lets KMP date the observation.

An exact retry with the same key replays and appends nothing; a different
document must use another key. A rejection writes nothing: read `feedback`.
A cross-about equivalence is declared only through `kmp_write_memory` with a
`kmp_relate` proposal, never by raw ingest.

More: `guide:kmp-agent:example:canonical-ingest`.
