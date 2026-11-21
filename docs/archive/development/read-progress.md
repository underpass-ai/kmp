# Making a pending read visible

In an earlier reader evaluation, one reader received two temporal entries and
left seven relations unread. The response carried `page.has_more:true` and a
continuation; the main message only said "Returned 2 temporal entries".
`selection.has_more:false` described the selection of entries, not the
projection of their proof. Another reader also stopped after a partial Wake.
Finding F-134 and [#544](https://github.com/underpass-ai/kmp/issues/544) keep
those reproductions and interviews.

## Decision

A successful but partial read starts its first MCP text with
`READ_INCOMPLETE`. The notice says that parts of the selected packet are
missing and that the reader must follow the returned action or explicitly keep
the partial result. The original summary follows. It is served without
requiring identity, context, guide consumption or extra persistence.

The signal is derived from the native fields, without interpreting prose:

- `page.has_more:true`;
- `projection.page.has_more:true`;
- `projection.core_text_shortened:true`, even when no expansion items remain.

The optional help and this notice use the same detection. The writer's own
neighbourhood signal keeps its scope; it is not confused with a read. A
temporal selection that offers more history but has already delivered all its
proof does not get the notice. Having `next_actions` is not used on its own
either: an action may start another selection. A complete UNKNOWN keeps its
meaning. The end of the pages never certifies semantic sufficiency or an
exhaustive search.

## Limit of the change

The text of the MCP envelope changes, not the canonical object.
`structuredContent`, its bytes, sources, clocks, omissions, indexes, cursors
and budgets stay the same. The main text adds a short notice, without
duplicating proof, complete actions or lessons. Clients that use only
`structuredContent` keep checking their native fields. No other persisted
state, no redundant completion field and no barrier controlling the LLM's
answer is added.

This aims to bring the existing signal to the reader sooner. It does not show
that an agent follows it; that is to be checked in a new task after delivery.
The closed evaluation attempts stay partial and are not repaired after the
fact.

## Acceptance

Freeze and replay real Wake and Goto calls on copies of the approved
evaluation write, without the trial's contexts, and complete only their
packets. Add Inspect, Trace, a complete UNKNOWN and a finished selection with
more history. Compare every structured page exactly before and after, and
measure the whole envelope. Also test a shortened core with no more pages, an
error, a call without registration and the consistency of the optional
indicator. Keep the summary text and the canonical content byte for byte.

Update the short entry, the temporal card, the extended guide, the budget
example and the human guide from their sources. Review the reduced native
surfaces and the response fixtures that change; do not add editorial CI
checks. Compression and the other requirements of the goal follow their own
separate tracking.

## Check result

V2 keeps exactly 22 pages and adds 16 notices; V3 checks another complete page
with more history available and no notice. That is 54 RPCs across the two
paired controls, with no errors and no models. V1 is kept with 10 RPCs and one
correct refusal, because the investigator asked for `page.entries` on Inspect;
V2 removes that argument. The V2 control named `goto-more-history` did not
produce that condition and is not used to prove it; V3 adds the documented
`limit.entries` without changing code or sources.

The notice adds 26 o200k tokens per partial page: 416 tokens and 2176 bytes in
V2, without changing `structuredContent`. The native catalogue and
initialization are unchanged. Agent entry: 359 → 389 tokens; temporal card:
328 → 433. No saving or new understanding is claimed. 613 MCP tests pass; one
earlier manual test stays skipped. Clippy and guide generation pass. The extra
shared_passages control checks the nine reads, exact expansion and the
consistency of the first text with the optional help. No editorial CI barriers
are added.
