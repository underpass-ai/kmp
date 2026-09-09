Start once with `kmp_guide {"registration_key":"unique-task-and-agent-key"}`. Save the returned agent and context ids.

Resume with `{"context_id":"<returned context_id>"}`. Add `"topic":"write"` to open one worked card; add `"fold":true` to hide it.

After losing context, use `{"agent_id":"<returned agent id>","context_key":"unique-reset-key"}` and save the new context id. Registration and reset keys identify logical operations: reuse them on retry.

KMP records what it served, not what you understood. A random name is a display label; use the returned ids. Extended details are in the returned next action.
