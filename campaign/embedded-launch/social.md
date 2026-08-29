# Launch posts

Publish each block as a two-post thread with the named MP4 attached to post 1.
Copy inside the block is exact; the evidence note is not social copy.

## Fresh process. Same why.

Attachment: `docs/assets/campaign/kmp-embedded/fresh-process-same-why.mp4`

Post 1:

> End the session. Keep the why.
>
> KMP Embedded keeps decisions, evidence and time in local SQLite. A fresh
> process opens the same store and recovers the same decision—with its evidence
> still attached.
>
> No account. No transcript dump.
>
> github.com/underpass-ai/kmp

Reply:

> What you're seeing is a real two-process test: Process 1 commits and exits.
> Process 2 starts with a different PID, opens the same local store and inspects
> the same ref.
>
> Local-first here means one machine and one store—not cloud sync.

Alt text: “A real terminal records a KMP decision, exits, then a fresh process
with a different PID recovers the same decision and evidence while ChronoLoom
shows the stored memory.”

Evidence note: require the promoted `fresh-process-same-why` capture and its
process/store/wire bindings before publication.

## Two processes. One memory.

Attachment: `docs/assets/campaign/kmp-embedded/two-processes-one-memory.mp4`

Post 1:

> Process A writes it. Process B recovers the why.
>
> Two independent KMP MCP processes. One local SQLite WAL store. No export. No
> import.
>
> The second process reads the decision and evidence the first committed.
>
> github.com/underpass-ai/kmp

Reply:

> The receipts behind the cut: distinct PIDs, same store fingerprint, exact MCP
> request/response, and ChronoLoom revisions observed in Chromium. The capture
> is OBS, not a terminal mockup.

Alt text: “Process A commits a pool-limit decision in a real terminal. Process
B, with a different PID and the same local SQLite WAL store fingerprint,
recovers it as ChronoLoom opens the same evidence.”

Evidence note: `Process A` and `Process B` are the only permitted identities.
Codex or Claude can replace them only after a new raw capture proves both real
hosts.

## Keep the wrong turn.

Attachment: `docs/assets/campaign/kmp-embedded/keep-the-wrong-turn.mp4`

Post 1:

> Delete the wrong turn. Lose the why.
>
> In this deterministic product fixture, KMP keeps the traffic-spike hypothesis,
> the evidence that contradicted it and the decision that replaced it.
>
> ChronoLoom lights the proof path.
>
> github.com/underpass-ai/kmp

Reply:

> The four hops in the video are stored relations: verified_by → supersedes →
> chosen_because → depends_on.
>
> No query language. No hunting through node IDs. The agent asks; the real
> browser follows over long-poll.

Alt text: “In a deterministic pool-saturation fixture, a real agent-directed
ChronoLoom view selects the configuration decision, shows its three recorded
times, then reveals four numbered proof hops from verification to the original
configuration change.”

Evidence note: always retain the fixture disclosure. This is not customer data
or a production incident.
