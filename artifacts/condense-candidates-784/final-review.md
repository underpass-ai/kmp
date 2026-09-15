# Review of #784 against main, 2026-09-14

Baseline: 4b97b15f79b450b8e3f589eb68d6fc8a20bbaab6 (v0.18.4).
Review scope: full feature diff against main and preserved uncommitted review fixes.

- The pure domain policy uses selected descriptors, card presentations and scope/time-admitted supports. It performs no port calls or body reads. Its priority is sharing descending, body bytes descending, ref ascending. Per-membership sets deduplicate shared support sources.
- Destination mode passes only selected returned routes. Seek passes groups only when relation clocks and candidate/group binding witnesses are known. This preserves the distinction from canonical proof delivery. The previous missing-witness defect is fixed and covered by public SQLite regressions.
- Card status takes precedence over the 1024-byte heuristic; valid, after-cut and below-floor counts are disjoint. Absent/stale expectations copy exact state; malformed stale stamps are not fabricated. The cap is eight and omitted_count counts only the eligible tail.
- Both protobuf copies and the domain/protobuf/MCP mapping agree. The new optional proof field is hashed by ReadSelectionFingerprint before pagination, while the canonical manifest remains unchanged. Existing unit controls explicitly mutate the field to verify invalidation.
- Public controls use the offered source and expectation to Condense, then exercise source changes and card CAS. Other controls cover language, cutoff, refused expansion, noncompact reads, selected routes and repeated pages. A real gRPC service compares encoded candidate messages across pages against embedded materialization.
- Reviewed the generated guide change structurally: content changes are limited to the agent Condense verb/card/reader-cards example and the human advanced reader-cards entry. Other entry changes are generated revision stamps. Actor/context guidance is preserved. Tool fixtures match the updated schema.
- Repeated the native reader-cards journey against a separately built main baseline with a private target. Body hashes match. Large/small journeys are 21 calls each; response/request totals are 260579→280645 and 101016→102876 bytes. Field-value costs are 19606/1400 bytes plus 460 bytes of field names/separators per journey. Native tools/list grows 244994→247172 bytes. No claim of token savings or agent understanding.
- The installed host remains on its installed plugin; this review establishes the candidate's native surface and generated assets, not a live upgrade of the current task's tool catalogue.

Validation evidence: guide-assets-final.log, tool-surface-final.log, replay-main.json, replay-main.log, quality-gate-main.log. The complete gate outcome is recorded in completion.md after the command exits.
