# ChronoLoom scene updates

Issue [#778](https://github.com/underpass-ai/kmp/issues/778) concerns work on the
browser main thread after a projection has arrived. It complements the server
projection cache; neither change replaces evidence retrieval or changes the
meaning of a clock, label, relation, selection or trace.

## Changes and ownership

The Three.js adapter retains node meshes and materials by exact ref. Position,
kind geometry, minimum screen size, selected emission and fading update in place.
The selection ring is shared. About planes, grid objects and their DOM labels
remain attached by about identity; obsolete layers dispose their own resources.
DOM label dimensions are read together before placement writes.

`loom-relation-lines.js` retains curve coordinates while endpoints stay fixed and
packs the existing 40-segment cubic curves into line buffers grouped by color and
opacity. Directional arrows still follow the original curve tangent at 0.96.
Hidden relations release curves, buffers and arrows. Replacing a GPU attribute
first disposes its former allocation; retained oversized buffers shrink when
capacity exceeds four times the required size. Shared glyph and arrow geometry
lives until the renderer is disposed. Disposal also cancels pending frames,
removes canvas listeners and disconnects controls and the resize observer.

Node instancing was evaluated against the actual scene's material and picking
requirements. Glyphs already share three geometries, but each node has independent
opacity, selected emission, aggregate scale and a ref used by ray picking.
Instancing would require buckets for these material states and an instance-to-ref
mapping, and would still update matrices when minimum screen size changes with
the camera. This delivery chooses shared edge buffers, which remove thousands of
per-edge allocations and draw calls while keeping the current node picking
contract. The remaining one-call-per-node cost is visible in the measurements;
node instancing remains a possible further optimization, not a claimed result.

Time-lens lookups use a binary search with the same inclusive boundary behavior
as the prior linear search. The inverse, compressed gaps and focus/context weights
remain unchanged. Navigator drags and hover retain only the newest input for each
animation frame; release flushes its last coordinate and keeps one undo record.
A replaced temporal extent cancels a pending old drag. Existing slider frame
coalescing is retained and tested; OrbitControls rendering is also coalesced.
Typing search waits 120 ms. Explicit search intents and Escape cancel pending
work and apply synchronously. The memory selector retains option identity,
updates edited captions and moves/removes only entries whose identities change.

## Reproduction

The runner uses actual bundled Three.js, WebGL and Chromium through Python
Playwright 1.58.0. It starts an isolated page with deterministic synthetic data;
it does not read a personal memory store. With that dependency and its Chromium
available:

```bash
python scripts/performance/chronoloom_scene.py artifacts/performance-778/before \
  --source-ref ec560926bc7dc46e7b973a65f806d6a2b6648a28 --entries 2048 --samples 6
python scripts/performance/chronoloom_scene.py artifacts/performance-778/after \
  --entries 2048 --samples 6
```

Use new output directories. Run the controls serially, without an overlapping
build, and retain every run. `--source-ref` reads renderer modules from that exact
Git revision; the same current driver measures both implementations. Outputs
contain module sources and SHA-256 hashes, browser/platform identity, per-update
measurements, a screenshot and a Chrome trace. Open the trace in Chromium's
performance tooling to inspect tasks, layout and garbage collection. A smaller
512-node control and the default 2,048-node graph both use two directed edges per
node. The scenarios change opacity, selection, window, camera mode and relation
visibility; they then clear, repopulate and dispose the scene.

`total_ms` includes layout, identity observation and renderer update. It is CPU
submission time, not GPU completion. `frame_interval_ms` observes consecutive
animation-frame timestamps around that work. Created-object, geometry and
material counts compare actual Three.js identities; they are not a general
JavaScript allocation profiler. The trace and heap observations provide
additional allocation/GC context. Chromium may quantize `performance.memory`;
CDP heap measurements are also recorded and no forced-GC baseline is claimed.

## Observed controls

Measurements use Chromium 145.0.7632.0 on Linux aarch64, an 1,280 × 900 viewport,
and ANGLE Vulkan **SwiftShader**. Software rendering does not establish hardware
GPU frame rates. The original reported 333/286/144 ms tasks were hypotheses;
the reproduced controls independently show main-thread work in the hundreds of
milliseconds. Results are informational and add no absolute CI timing gate.

The final measurement table is recorded below after validation. Early controls
under `baseline-*`, `candidate-*`, `final-*` and `measured-*` include concurrent
local gate activity or evolving instrumentation; retain them as development
observations rather than treating them as the final controlled comparison.

## Correctness and limits

The Node suite checks indexed lenses against the linear oracle at every segment
boundary, inverse boundaries, infinities/NaN and focus/context modes. A counted
lookup checks logarithmic work without a wall-clock assertion. Interaction tests
cover bursts, last release coordinates, cancellation, extent replacement, undo,
edge resizing, fresh windows, clicking, slider state, selector identity and
search/intent/Escape ordering. Existing scene-model tests retain clock, focus,
layer, filter, camera and visible-endpoint coverage.

The browser runner additionally checks real ray picking in both camera modes,
reference callbacks, coalesced hover, labels retained and removed with their
layers, dimming, theme/selection changes, trace arrow direction, endpoint changes,
resource plateaus across eight clear/repopulate cycles, disposal and silent
listeners after disposal. HTTP and MCP App use the same registered module source;
the full native gate checks their asset and capability contracts.

The changes reduce object churn and CPU submission substantially. They do not
promise a long-task-free dense scene: software GPU rendering, the remaining node
draw calls, layout work, first construction and revealing previously hidden
relations can still block a frame. Per-scene retained state follows the current
visible graph; it is released when entries/layers/relations leave or the renderer
is disposed. No evidence or response cache is introduced in the browser.
