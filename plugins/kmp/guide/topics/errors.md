## When the tools are not there

If the kmp tools are missing from your inventory, do not silently
fall back to re-deriving everything — say so. The usual causes are specific
and fixable, and `/kmp:doctor` distinguishes them:

- the `kmp-mcp` binary is not installed or not on `PATH`;
- the session started before the binary, plugin, or MCP registration changed,
  so it is still carrying the old tool inventory — restart the session;
- the configured binary or data directory no longer exists — run
  `/kmp:doctor` to identify the stale path;
- the host has the MCP registered and enabled and has never connected to it.
  A registration in a host's inventory is not a conversation that can call a
  tool. `/kmp:doctor` reports that state as *unverified* rather than usable,
  and reports it separately from the plugin being installed and from the
  engine binary declaring its tools when run directly.

Which memory opens is a separate question from whether the tools are there,
and it has one order: the `KMP_MCP_DATA_DIR` environment variable, then the
nearest project root, then the selection saved with
`kmp-mcp config memory-store <absolute-path>`, then the per-user default.
`kmp-mcp config` prints the saved selection, the effective one and which rule won, without starting a host
or creating a store. A valid explicit override still works if the saved
selection is invalid; config and doctor report that unused setting for repair.
Without an override, repair or clear the invalid selection before starting
memory. Quoted paths may contain `#`; it is part of the directory name.
Memory this engine cannot open is refused with its reason:
never migrate, move or overwrite such a store, and never propose that a user
do so as a repair. Selecting another directory is their decision.

## Errors

Tool failures set `isError=true` and carry
`structuredContent.error.{code,message}`. Read the code; it is produced where
the failure happened, never inferred from the words, and `tools/list` carries
the closed set under `_meta."kmp/errorCodes"` with what each one means.

| code | what to do |
| --- | --- |
| `invalid_argument` | fix the arguments — retrying unchanged cannot work |
| `not_found` | the requested about or ref was not found; check the exact address and selected store before inferring absence |
| `conflict` | do not replay unchanged: a key accepted with different content must not be reused, and a moved view or a guard needs a decision; follow the message |
| `retryable_conflict` | the store moved under the write and nothing was applied; the server already retried it, and replaying the same call with the same key is safe |
| `unavailable` | the kernel was unreachable; the same call may work later |
| `unknown_tool` | no such tool here |
| `backend_error` | the kernel failed for a reason no argument can fix |

Every call is checked against the tool's published input schema before
anything reads it: an unknown argument, a wrong JSON type, a value outside an
`enum`, a number past its bounds or a missing required argument is refused as
`invalid_argument` with a `feedback` item naming the `field` (`SCHEMA_VIOLATION`
or `UNKNOWN_ARGUMENT`), never answered from defaults. Clock arguments need a
zone: `Z` or an offset such as `+01:00`.

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

No automatic sync or retry occurs (the first read of a guide node in a session
seeds the installed guide into the store, once). Backend/unavailable errors and
failed reads of the agent guide do not suggest reading the guide again. An Ask
UNKNOWN is a successful semantic result, not an input error or a trigger for
this help. First-use discovery is an instruction to the agent; the server does
not persist a claim that the agent has learned a verb.
