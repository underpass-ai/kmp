Resume known work with `kmp_wake {"about":"project:sample"}`. Use the exact
about from the task or a prior result. Read its label catalogue before choosing
filters or writing new labels.

Expect context, selected proof, a resume cursor and continuation guidance. `scope`
separates selected proof from about context: time bounds apply to the
selection, while `summary`, `current_state`, semantic next actions and labels
can include other times; requested dimensions apply to both. Do not use an
unbounded next-action summary as proof of the selected historical state.

Execute `projection.next_action` until the relevant packet is complete. Then
navigate history when the task asks what happened, or inspect a relied-on ref.
Missing memory means there is nothing known in that selection; do not invent it.
`page.entries` counts projection records (a label, a path step, an evidence
item, a state line), not memories; `page.total` counts the same records.

More: `guide:kmp-agent:example:shared-resumption` and
`guide:kmp-agent:example:budget-proof`.
