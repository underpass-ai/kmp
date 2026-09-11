# Execution and its later verification

Fictional source X3, received on 2026-09-04 at 12:00 UTC: R8 ran in staging under
P2 at 11:00. H8 restored the test file and verified its checksum at 11:05,
confirming that R8 was recoverable. The source does not establish when
recoverability began.

Store the execution and later check separately, even though one report describes
both. Keep each summary and its evidence fragment limited to that event; retain
X3 as provenance. Do not put “already recoverable” into the 11:00 execution.
Both records may share observed_at=12:00. Neither needs an invented validity date.
The verified_by link points from the execution outcome to the check; its rationale
qualifies what that later check proves. It does not make the check occur at 11:00.
The link is declared with observation 12:00 and the kernel's actual ingestion.
Its own occurrence and validity remain absent: do not copy 11:00 or 11:05 into
it. On observed reads before noon it is excluded. On occurred reads its own
clock is unknown; the endpoint selection must still exclude the later check.
ChronoLoom's relation detail shows these dates separately from the nodes.

Use an isolated teaching store. The ${...} notation copies the exact returned
value, never a reconstructed ref. Send only each call's arguments.

```json
{
  "tool": "kmp_wake",
  "save_as": "initial",
  "expect_error": "not_found",
  "arguments": {
    "about": "example:guide:execution-verification",
    "budget": {
      "detail": "compact",
      "max_bytes": 10000
    }
  }
}
```

```json
{
  "tool": "kmp_write_memory",
  "save_as": "pending",
  "arguments": {
    "about": "example:guide:execution-verification",
    "actor": "guide-writer",
    "source_kind": "human",
    "observed_at": "2026-09-04T12:00:00Z",
    "idempotency_key": "guide-execution-verification:v1",
    "labels": {
      "source": [
        "X3"
      ],
      "backup": [
        "R8"
      ],
      "environment": [
        "staging"
      ]
    },
    "memories": [
      {
        "id": "execution",
        "kind": "observation",
        "occurred_at": "2026-09-04T11:00:00Z",
        "summary": "R8 ran in staging at 11:00 under authorization P2.",
        "evidence": "X3 execution log: R8 ran in staging at 2026-09-04T11:00:00Z under authorization P2.",
        "connect_to": [
          {
            "ref": "check",
            "rel": "verified_by",
            "why": "H8 tests recovery of the backup produced by R8; this later check verifies that outcome, not the onset of recoverability.",
            "evidence": "X3 reports that H8 at 11:05 restored the test file and verified its checksum, confirming R8 recoverability.",
            "confidence": "high"
          }
        ]
      },
      {
        "id": "check",
        "kind": "observation",
        "occurred_at": "2026-09-04T11:05:00Z",
        "summary": "At 11:05 H8 restored the test file and verified its checksum, confirming R8 recoverability.",
        "evidence": "X3 independent check: H8 at 2026-09-04T11:05:00Z restored the test file and verified its checksum; H8 confirms R8 is recoverable.",
        "labels": {
          "check": [
            "H8"
          ]
        }
      }
    ]
  }
}
```

Review needs_review: nothing has been saved. Both proposed records are available
in the original arguments. Resume after reviewing the text, clocks and link.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "written",
  "arguments": "${pending.next_actions.0.arguments}"
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "execution_read",
  "arguments": {
    "about": "example:guide:execution-verification",
    "ref": "${written.local_refs.execution}",
    "budget": {
      "max_bytes": 16000
    }
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "check_read",
  "arguments": {
    "about": "example:guide:execution-verification",
    "ref": "${written.local_refs.check}",
    "budget": {
      "max_bytes": 16000
    }
  }
}
```

## Compare the clocks at a fine cutoff

Occurred at 11:02 admits the execution, not H8. Occurred at 11:05 admits both.
Observed at 11:02 admits neither: this report arrived at noon. Observed at noon
admits both. Complete relevant pages and keep dependency omissions explicit.
A full current Inspect can show the later link and its rationale; it is not a
historical selection. Do not replace the selected temporal proof with that panel.

```json
{
  "tool": "kmp_goto",
  "save_as": "occurred_before",
  "arguments": {
    "about": "example:guide:execution-verification",
    "axis": "occurred",
    "at": {
      "time": "2026-09-04T11:02:00Z"
    },
    "include": {
      "dependencies": true
    },
    "limit": {
      "entries": 100
    },
    "budget": {
      "max_bytes": 24000
    }
  }
}
```

```json
{
  "tool": "kmp_goto",
  "save_as": "occurred_after",
  "arguments": {
    "about": "example:guide:execution-verification",
    "axis": "occurred",
    "at": {
      "time": "2026-09-04T11:05:00Z"
    },
    "include": {
      "dependencies": true
    },
    "limit": {
      "entries": 100
    },
    "budget": {
      "max_bytes": 24000
    }
  }
}
```

```json
{
  "tool": "kmp_goto",
  "save_as": "observed_before",
  "arguments": {
    "about": "example:guide:execution-verification",
    "axis": "observed",
    "at": {
      "time": "2026-09-04T11:02:00Z"
    },
    "include": {
      "dependencies": true
    },
    "limit": {
      "entries": 100
    },
    "budget": {
      "max_bytes": 24000
    }
  }
}
```

```json
{
  "tool": "kmp_goto",
  "save_as": "observed_after",
  "arguments": {
    "about": "example:guide:execution-verification",
    "axis": "observed",
    "at": {
      "time": "2026-09-04T12:00:00Z"
    },
    "include": {
      "dependencies": true
    },
    "limit": {
      "entries": 100
    },
    "budget": {
      "max_bytes": 24000
    }
  }
}
```

```json
{
  "tool": "kmp_trace",
  "save_as": "path",
  "arguments": {
    "about": "example:guide:execution-verification",
    "from": "${written.local_refs.execution}",
    "to": "${written.local_refs.check}",
    "budget": {
      "max_bytes": 16000
    }
  }
}
```

## See the separate events in ChronoLoom

Select R8 in the narrow occurred window, then expand to H8. The current inspector
may show later links outside that window. The scene and the historical MCP query
have separate selection state. A viewer gesture does not change saved clocks.

```json
{
  "tool": "kmp_view_open",
  "save_as": "view",
  "arguments": {
    "about": "example:guide:execution-verification"
  }
}
```

```json
{
  "tool": "kmp_view_apply_intent",
  "save_as": "before_frame",
  "arguments": {
    "view_id": "${view.view_id}",
    "expected_revision": "${view.view_revision}",
    "idempotency_key": "guide-event-separation:before_frame",
    "focus": {
      "time_range": {
        "from": "2026-09-04T10:59:00Z",
        "to": "2026-09-04T11:03:00Z",
        "axis": "occurred"
      }
    },
    "selection": "${written.local_refs.execution}",
    "projection": {
      "semantic_zoom": "moment"
    },
    "explanation": "Review separate occurrence times for R8 and H8"
  }
}
```

```json
{
  "tool": "kmp_view_apply_intent",
  "save_as": "after_frame",
  "arguments": {
    "view_id": "${view.view_id}",
    "expected_revision": "${before_frame.state.view_revision}",
    "idempotency_key": "guide-event-separation:after_frame",
    "focus": {
      "time_range": {
        "from": "2026-09-04T10:59:00Z",
        "to": "2026-09-04T11:06:00Z",
        "axis": "occurred"
      }
    },
    "selection": "${written.local_refs.check}",
    "projection": {
      "semantic_zoom": "moment"
    },
    "explanation": "Review separate occurrence times for R8 and H8"
  }
}
```

```json
{
  "tool": "kmp_view_get_state",
  "save_as": "view_state",
  "arguments": {
    "view_id": "${view.view_id}"
  }
}
```

This authored replay proves the native cutoff behavior for this representation.
Only a fresh writer check can show whether the example improves agent use.
