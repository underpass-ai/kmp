Open the exact about in ChronoLoom:
`kmp_view_open {"about":"project:sample"}`.
Keep the returned view id and revision. Get shared state with
`kmp_view_get_state {"view_id":"<returned view id>"}`.

The view is read-only with respect to memory. Use `kmp_view_apply_intent` for
semantic framing, clock, zoom, selection and filters; consult its linked verb
before constructing a new intent. An intent is atomic and idempotent under
optimistic revisions: if the person at the loom moved first it conflicts, and
you rebase rather than overwrite their framing. A ref the store lacks is dropped
from the part that named it and listed in `unhonored`; when none exists the
view does not move and `applied` is false. Never pixels, coordinates or code.

Use the prism to compare clocks and dimensions, and inspect a selected memory's
source evidence and relations. A current-record inspector can include changes
outside the scene window; it is not by itself a historical answer. Hidden labels
or an off-window endpoint can hide an edge without deleting the stored relation.

More: `guide:kmp-agent:example:shared-resumption` and
`guide:kmp-agent:example:dimensional-memberships`.
