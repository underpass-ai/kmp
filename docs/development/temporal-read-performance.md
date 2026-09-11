# Structured temporal reads

Temporal navigation and ChronoLoom projections consume the structured memory
bundle. They resolve the same about roots and dimension scopes, read the same
graph and details, merge abouts in the same order and apply the same temporal
admission and response projection. They do not need an intermediate rendered
prompt or its tokenizer and tier calculations.

`QueryApplicationService::read_context_bundle` reuses the existing rehydration
reader without rendering or saving a snapshot. The ordered bundle union is
shared with rendered reads. The removed `TemporalMemoryResult.quality` field
described the discarded prompt; MCP response quality is computed separately
from the selected entries and relations. The local prompt-quality journal no
longer emits those unused temporal render metrics. It does not replace them
with zeros. Wake, Ask, Relate and Trace keep their rendered contexts.

This is an internal read-path optimization. It changes no temporal arguments,
entry selection, dependency limits, source proof, page semantics or agent guide
instructions. It introduces no writer field or new CI gate.

Validation uses complete before/after MCP response equality on copied stores,
including evidence, relation direction, clocks, selection, continuation actions
and response quality. Timing includes serialization and transport. Native tests
cover temporal selection and proof, fields, pages, multiple dimensions and
ChronoLoom projections. Local performance measurements are exploratory; they
do not establish benchmark answer accuracy or an engine-wide speedup.

Remaining work in #539: graph/detail snapshot consistency and repeated Inspect
reads. The underlying ports and number of storage reads are unchanged here.
Directed navigation still needs a bound on discovery, not just on the nodes
selected after reading a full catalogue (#538).
