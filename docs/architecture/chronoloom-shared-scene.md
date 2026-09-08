# ChronoLoom: about layers and shared control

ChronoLoom has one memory scene. Each about owns a plane; labels select
memories and remain ordinary attributes. Perspective orbit and an orthographic
flat view are two cameras over the same scene, clock, window and selection.
Neither labels nor camera coordinates become memory semantics.

## Boundaries

| Responsibility | Owner | Dependencies |
| --- | --- | --- |
| Revision, provenance, handoff and undo invariants | `view/domain/ViewSession` | Value objects only |
| Execute a human handoff and notify readers | `view/application/use_cases/TakeViewControl` | Session store, change bell, wall clock ports |
| HTTP and negotiated MCP App commands | View adapters | Application DTOs/use cases |
| Bounded projection reads and stale-response rejection | Browser data use cases | Backend port; no WebGL or DOM |
| About ownership, strict clock placement and scene topology | Pure scene model | Projection values; no browser APIs |
| Camera, meshes, picking and resource disposal | Three.js adapter | Scene model; selection callback |
| Human/agent status, explanation and undo | DOM adapter | Authoritative view snapshot |
| Pending content reads and superseded completion tokens | Browser activity tracker | State publisher; no DOM, clock or transport |
| Loading status, busy content and paint transitions | DOM adapter | Activity snapshots and animation frames |

The existing view aggregate remains the authority. “Human” and “agent” identify
the last actor, not an exclusive lock or a promise that an agent is connected.
Taking control is an explicit revision-checked operation: it preserves the
semantic frame and changes provenance. Merely reasserting an unchanged ordinary
intent remains a no-op. Undo retains its existing history semantics.

Loading is transient browser activity, not part of the shared view aggregate.
The backend port brackets content reads in both HTTP and MCP Apps; view polling
and control commands do not create loading activity. Layer changes also bracket
their extent and projection work so adding or removing a plane keeps one status
through chained reads. Completion tokens prevent superseded requests from
clearing newer activity. The DOM adapter settles after the next paint, fades
the content and respects reduced motion; no artificial wait is added to reads.

## Read budget and evidence

The renderer consumes the existing atlas/episode/moment projection ladder. The additional-about value object limits the scene to five
additional reads plus its primary context. Human layer and detail reports
replace only their facets, preserving the agent’s overlays and label predicates.
Bounded episode probes establish the union of the selected abouts' extents,
including disjoint dates or a primary about empty on the chosen clock.
Atlas reads over that union populate the brush; broad extent-probe bins are
never stretched into a misleading density strip.
Adding layers expands an All window and preserves an explicitly focused window.
Aggregates remain aggregates and partial projections remain visibly partial.
No full-store export, private preview snapshot, generated placeholder edge or
fixed label taxonomy belongs in the shipped UI. Inspector and audit paths keep
using the existing read ports. Browser and MCP App embed the same assets without
CDN access, runtime package installation or a second memory write path.

At Moment, a projection carries the equivalences declared by its returned
source memories, including a target ref in another about. Those declarations
come from the same bounded context read, before dimension filtering removes
the foreign endpoint. They retain why, evidence, confidence and the writer's
proposal method; no proposal or inferred coincidence becomes an edge. A source
outside the selected clock, range, labels or page contributes no declaration.
The reference does not load its target: the scene draws the arc only if both
endpoints are present in the independently requested planes. Removing or
filtering a plane removes that endpoint and its arc. Projection relation
metrics count returned proof edges; the scene counts only edges it can draw.

## Verification

`loom-*.test.js` runs in the required GitHub Actions test job as well as the
local quality gate. Its fixtures are synthetic; the approved prototype's
private snapshot is excluded from the repository.

Domain tests cover handoff, unchanged frame, stale revision, repeated handoff
and undo. Application/transport tests prove both adapters reach that aggregate.
Browser tests cover stale loads, exact clocks, labels, common time windows and
actual perspective/orthographic camera geometry. The normal contract,
architecture, documentation, lint, test and coverage gates remain required.

The label-selection contract is based on the existing projection work in
PR #527. This change does not migrate KMP's stored labels or establish preferred
label names.
