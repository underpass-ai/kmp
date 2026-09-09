## When the tools are not there

If the kmp tools are missing from your inventory, do not silently
fall back to re-deriving everything — say so. The usual causes are specific
and fixable, and `/kmp:doctor` distinguishes them:

- the `kmp-mcp` binary is not installed or not on `PATH`;
- the session started before the binary, plugin, or MCP registration changed,
  so it is still carrying the old tool inventory — restart the session;
- the configured binary or data directory no longer exists — run
  `/kmp:doctor` to identify the stale path.

## Errors

Tool failures set `isError=true` and carry
`structuredContent.error.{code,message}`. Read the code; it is produced where
the failure happened, never inferred from the words, and `tools/list` carries
the closed set under `_meta."kmp/errorCodes"` with what each one means.

| code | what to do |
| --- | --- |
| `invalid_argument` | fix the arguments — retrying unchanged cannot work |
| `not_found` | the requested about or ref was not found; check the exact address and selected store before inferring absence |
| `conflict` | inspect the message: retryable concurrency conflicts are safe to replay with the same key; a key accepted with different content must not be reused |
| `unavailable` | the kernel was unreachable; the same call may work later |
| `unknown_tool` | no such tool here |
| `backend_error` | the kernel failed for a reason no argument can fix |

An unknown argument is refused rather than dropped: every tool declares
`additionalProperties: false` and the boundary enforces it, so a misspelling
comes back naming the key instead of being answered from defaults.

Usage refusals also offer `structuredContent.help.guide` and `help.examples`:
complete `{tool, arguments}` calls to the relevant verb and worked lessons.
Read the field and code in every feedback item first; reuse lessons already in
context and consult only the missing explanation. The help is also available in
the text fallback. It does not replace `feedback[].action`, which can inspect
a missing prerequisite or restart a stale selection. Neither help nor a lesson
authorizes writing invented evidence or widening the task's scope.

Example: a `FUTURE_OBSERVATION` refusal links to the four-clock lesson. Read the
source to correct the observation time; do not substitute today's time merely
to make the call pass. A `RELATION_PROOF_REQUIRED` refusal links to decision
history, where the relation has its own why and evidence. If the source does not
justify that relation, the lesson cannot supply its proof.

No automatic guide read, sync or retry occurs. Backend/unavailable errors and
failed reads of the agent guide do not suggest reading the guide again. An Ask
UNKNOWN is a successful semantic result, not an input error or a trigger for
this help. First-use discovery is an instruction to the agent; the server does
not persist a claim that the agent has learned a verb.
