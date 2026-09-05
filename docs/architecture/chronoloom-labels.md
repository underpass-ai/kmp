# ChronoLoom and labels — the value on screen

The label series ([#507](https://github.com/underpass-ai/kmp/pull/507) to
[#514](https://github.com/underpass-ai/kmp/pull/514), v0.12.0) taught the
kernel that a label is a `key = value` pair with a catalogue, selectors and a
relabel. ChronoLoom was left where it was: it draws one lane per **key** and
inside a lane every entry is the same. The value travels in the payload, is
computed in the browser, and is thrown away. This is the checked-in target
for closing that gap, in the tradition of
[chronoloom-layer-map.md](chronoloom-layer-map.md): the map before the code,
one slice at a time, no behaviour change smuggled inside a refactor.

## What was lost

In the study's terms the memory is a fibration: the base is the set of
`(key, value)` pairs, and the fibre over a pair is the set of entries standing
in it. A lane per key draws the base **collapsed onto the key** — the union of
every fibre with that key. That is why three lanes of the same about look
identical: they are three projections of one set of entries. What the lane
loses is the partition, and the partition is where everything the series added
lives: a sequence is a counter per fibre, `relate` pairs by fibre and shares
its cap per fibre, a selector is a predicate over the pairs, and `kmp_relabel`
moves an entry between fibres and leaves its why on the edge.

The data path already carries the pair. It is dropped in four places:

| Where | What happens |
|:--|:--|
| `kmp-application` `visual_projection.rs` | An entry is positioned by `coordinates.map(dimension)`; bins and clusters are keyed by key only. |
| `ui/loom-core.js` `buildLanes` | Computes `lane.scopes` (value → count); nothing renders it. |
| `ui/loom-data.js` | At Atlas and Episode there are no entries, so lanes come from bins and clusters, which carry no value at all. |
| `kmp-viewer` `query_params.rs` | Reads `dims` and `scope`; `DimensionSelection` already carries `scope_ids` and `selectors`. |

## The representation

**The vertical axis is the catalogue, two levels deep.** A lane is a key, as
today; a row inside it is a value — one fibre. A folded lane is exactly
today's picture. Rows come from the about's catalogue, not from the window, so
a label with nothing in this range is drawn as an empty row (`0 · N`) rather
than not at all. An entry standing in three labels is one mark per fibre,
joined by its braid: the repetition was never the problem, the indistinction
was. Colour stays the kind; the value is a position, never a hue.

Rows are budgeted, never scrolled: rows are shared out among lanes in
proportion to their values, a lane that gets fewer rows than values shows its
most used ones and one `+n more` row, and the rail lists every label the
catalogue holds with its counts. Nothing is hidden without saying so.

**Coincidence instead of echo.** Focusing a fibre emphasises its entries and
shows every other fibre's intersection with it — `relate`'s comparability by
fibre, on screen.

**Selectors without a query builder.** Row and lane visibility (hide, fold,
pin) is coordinate-level and stays in the browser. Selectors are entry
predicates, travel to the kernel, and are the four the domain has: clicking a
value or key with a modifier adds `in`, `notin`, `exists` or `notexists` as a
chip; the chips are view state (`projection.labels`) on both faces, with the
same `{ key, op, values }` shape as `dimensions.selectors`, and a key the
catalogue lacks is reported as unhonored.

**The relabel is visible.** The `contains_entry` edge a relabel adds carries
`method: kmp_relabel`, the why, and who did it when. The coordinate now
carries that origin, so the stitched mark is drawn distinctly, the tooltip
quotes the why, and the entry's record lists each label with its origin.

## Layer map

### `kmp-domain`

| Concept | Where |
|:--|:--|
| `CoordinateOrigin` — method, rationale, motivation off the edge; empty for a label given at write | `value_objects/coordinate_origin.rs` |
| `TemporalCoordinate.origin()` | `value_objects/temporal_coordinate.rs` |

### `kmp-application`

| Concept | Where |
|:--|:--|
| `VisualLabel` — one label of the about with `entries` (all time) and `in_range`; `catalogue(bundle)` reads it off the `contains_entry` edges before the read's filter narrows them | `memory/visual_label.rs` |
| `VisualBin.scope_id`, `VisualCluster.scope_id`; `TemporalCoordinateView.method / why / motivation`; `VisualProjectionResult.labels` | `memory/visual_projection.rs` |
| `temporal_read` + `temporal_result`: the context before and after its filter, so the projection reads the catalogue whole | `memory/service.rs` |

### Contract (additive)

`TemporalCoordinate.method / why / motivation`, `VisualBin.scope_id`,
`VisualCluster.scope_id`, `VisualLabel`, `ProjectVisualResponse.labels`.
Mapped in `kmp-proto-mapping`, rendered in `kmp-mcp`'s
`projection/visual_projection.rs` and `rendering.rs`, pinned by the re-blessed
tool-surface fixtures.

### `kmp-viewer` — Rust

| Concept | Layer | Where |
|:--|:--|:--|
| `labels` and `scope_ids` query parameters, the same grammar for the projection read and the human's report | adapter | `query_params.rs` |
| `LabelSelection` (key, `LabelOperator`, values) — the closed vocabulary | domain | `view/domain/label_selection.rs`, `label_operator.rs` |
| `ProjectionSettings.labels`; `ViewPatch.projection_labels` (a person's chips, beside `focus_window`, so a report never clears an agent's overlays) | domain | `view/domain/projection_settings.rs`, `view_patch.rs`, `view_state.rs` |
| `LabelSelectorDto`, `ProjectionDto.labels`, `ViewIntentDto.projection_labels` | dto | `view/application/dto/` |
| vocabulary refusals both ways | mappers | `view/application/mappers/` |
| `CoordinateView.method / why / motivation`; label times made readable | wire | `views.rs` |

### `kmp-mcp` — the intent

`kmp_view_apply_intent` gains `projection.labels` with the selector shape;
`view_tools.rs` parses it, and `view_dispatch.rs` resolves its keys and values
against the projection's catalogue the way it resolves dimensions, reporting
what the about does not hold as unhonored.

### `ui/` — the browser

| Concept | Where |
|:--|:--|
| `foldAggregates` — a lane folded: per-label bins summed, clusters' refs merged | `loom-core.js` |
| lanes and fibres from `labels[]`; the row budget; fibre overlap; the selector grammar | `loom-core.js` |
| `view.foldedLanes`, `view.focusFibre`, `view.selectors`; `visibleRows()` | `loom-state.js` |
| rows per fibre at every rung; the braid across rows; the stitched mark | `loom-scene.js` |
| the two-level rail, the chips, the Labels section of the record | `loom-panels.js` |
| `projection.labels` in the snapshot and in the report | `loom-sync.js` |

## Slices

| # | Slice | Proof |
|:--|:--|:--|
| 1 | Kernel and contract: the pair on bins and clusters, the catalogue, the origin on the coordinate; the browser folds until rows land | application tests; the HTTP smoke with a real relabel; contract gate; re-blessed fixtures — landed in [#527](https://github.com/underpass-ai/kmp/pull/527) |
| 2 | Adapters, view context and intent: `labels` on the wire, `LabelSelection` in the domain, `projection.labels` in the tool with unhonored keys | crate tests; tool-surface parity |
| 3 | The browser: rows per fibre, the rail, chips, focus, the stitch, the Labels section | `node --test`; MCP App contracts; a browser pass over a copy of the real store |
| 4 | Docs: this map closed, the viewer README, the skill and guide | documentation spine |

A fibre's identity is the pair, never the value alone: the real store holds
two values that stand under two keys each (`project:kmp:launch-campaign-0.6.1`
under `task` and under `agentic_process`), written before v0.12.0 fixed one
key per value. They are two rows, and focusing one counts only its own
entries.

Out of scope, on purpose: the 2.5D "about as depth" prototype (the backend is
one about per projection), and the Occurred axis dropping entries that carry
no time on it, which is its own issue.
