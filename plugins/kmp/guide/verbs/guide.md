Start one agent with `kmp_guide {"registration_key":"unique-task-and-agent-key"}`.
Keep the returned agent id, random name and context id. Repeat that registration
key only for the same logical agent; another agent needs another key.

Resume with `{"context_id":"<returned context_id>"}`. Expand a worked card with
`{"context_id":"<returned context_id>","topic":"write"}`. Fold it with the same
arguments plus `"fold":true`. Folding keeps its delivery record; asking again
serves the card again. Extended verb guidance is in `next_actions`.

After compaction or a task change, use
`{"agent_id":"<returned agent id>","context_key":"unique-reset-key"}`. Keep the
new context id. The agent keeps its identity and random name but starts with no
guidance served in that context. Repeating the reset key resumes that new context.
A guide revision change also starts a fresh view of served topics; older delivery
records remain in storage.

These are delivery records, not proof that you understood or still retain a
lesson. They grant no additional permissions. Different agents sharing a host
or connection keep different identities. KMP never infers identity from the
connection. Preserve these ids in the host's task state; do not substitute the
display name. `durable=false` identifies an in-memory test backend, not persistence.
