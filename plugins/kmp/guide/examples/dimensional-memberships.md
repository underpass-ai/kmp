# Multiple values, distinct facets and late labels

These fictional sources establish the whole case:

- R1, observed September 1 at 10:00 UTC: the registry lists **neb** and
  **Nébula Cache** as aliases of component **neb**.
- R2, observed September 2 at 10:00 UTC: the registry retires the short alias
  **neb**, adds **NC** and **Nebula service**, and keeps **Nébula Cache**.
  The component identifier **neb** is unchanged.

Use a fresh teaching store. Read the write verb and scope topic. Send only
`arguments`; `${...}` copies an exact returned value. This is a native
contract example, not an evaluation of an independent LLM.

## Write the memberships in one call

Arrays are required even for one value. R1 supports both alias memberships;
`component=neb` is a separate facet. These memberships do not declare that
another record or another about with the word neb is the same entity.

```json
{
  "tool": "kmp_write_memory",
  "save_as": "original",
  "arguments": {
    "about": "example:guide:dimensional-memberships",
    "actor": "guide-writer",
    "source_kind": "human",
    "labels": {
      "alias": [
        "neb",
        "Nébula Cache"
      ],
      "component": [
        "neb"
      ],
      "agentic_process": [
        "registry"
      ]
    },
    "occurred_at": "2026-09-01T10:00:00Z",
    "observed_at": "2026-09-01T10:00:00Z",
    "idempotency_key": "guide-dimensional:original",
    "options": {
      "strict": false
    },
    "memories": [
      {
        "id": "current",
        "kind": "observation",
        "summary": "R1: The registry lists neb and Nébula Cache as aliases of component neb.",
        "evidence": "R1: The registry lists neb and Nébula Cache as aliases of component neb."
      }
    ]
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "before",
  "arguments": {
    "about": "example:guide:dimensional-memberships",
    "ref": "${original.generated_refs.0}",
    "include": {
      "raw": true
    },
    "budget": {
      "max_bytes": 30000
    }
  }
}
```

Expect four memberships: the process, two aliases and the component. Copy
canonical refs from the response when a graph operation needs an address.
One repeated pair would be rejected; a reused value under a different key
is valid. Matching both aliases returns the memory once.

```json
{
  "tool": "kmp_goto",
  "save_as": "two_aliases",
  "arguments": {
    "about": "example:guide:dimensional-memberships",
    "at": {
      "time": "2026-09-01T10:00:00Z"
    },
    "dimensions": {
      "selectors": [
        {
          "key": "alias",
          "op": "in",
          "values": [
            "neb",
            "Nébula Cache"
          ]
        }
      ]
    },
    "budget": {
      "max_bytes": 30000
    }
  }
}
```

## Apply the late catalogue correction

R2 changes memberships; it does not rewrite the source memory or move its
occurrence date. The response names what was added, removed and remains.
Only `alias=neb` is removed. `component=neb` still exists.

```json
{
  "tool": "kmp_relabel",
  "save_as": "change",
  "arguments": {
    "about": "example:guide:dimensional-memberships",
    "ref": "${original.generated_refs.0}",
    "actor": "guide-writer",
    "source_kind": "human",
    "observed_at": "2026-09-02T10:00:00Z",
    "add": {
      "alias": [
        "NC",
        "Nebula service"
      ]
    },
    "remove": {
      "alias": [
        "neb"
      ]
    },
    "why": "R2 retires alias neb, adds NC and Nebula service, and keeps the component identifier neb.",
    "idempotency_key": "guide-dimensional:relabel"
  }
}
```

```json
{
  "tool": "kmp_inspect",
  "save_as": "after",
  "arguments": {
    "about": "example:guide:dimensional-memberships",
    "ref": "${original.generated_refs.0}",
    "include": {
      "raw": true
    },
    "budget": {
      "max_bytes": 40000
    }
  }
}
```

```json
{
  "tool": "kmp_goto",
  "save_as": "retired_alias",
  "arguments": {
    "about": "example:guide:dimensional-memberships",
    "at": {
      "time": "2026-09-01T10:00:00Z"
    },
    "dimensions": {
      "selectors": [
        {
          "key": "alias",
          "op": "in",
          "values": [
            "neb"
          ]
        }
      ]
    },
    "budget": {
      "max_bytes": 30000
    }
  }
}
```

```json
{
  "tool": "kmp_goto",
  "save_as": "component",
  "arguments": {
    "about": "example:guide:dimensional-memberships",
    "at": {
      "time": "2026-09-01T10:00:00Z"
    },
    "dimensions": {
      "selectors": [
        {
          "key": "component",
          "op": "in",
          "values": [
            "neb"
          ]
        }
      ]
    },
    "budget": {
      "max_bytes": 30000
    }
  }
}
```

```json
{
  "tool": "kmp_goto",
  "save_as": "new_alias",
  "arguments": {
    "about": "example:guide:dimensional-memberships",
    "at": {
      "time": "2026-09-01T10:00:00Z"
    },
    "dimensions": {
      "selectors": [
        {
          "key": "alias",
          "op": "in",
          "values": [
            "NC"
          ]
        }
      ]
    },
    "budget": {
      "max_bytes": 30000
    }
  }
}
```

The retired-alias read is empty. The component and new-alias reads each
return the original memory at September 1. The added labels inherit its
clocks; the relabel edge records R2's rationale and later receipt separately.
An empty alias result does not prove the source was deleted. Inspect its ref.

## Review in ChronoLoom

Open the original memory with every facet visible. It should have three alias
values and one component value, plus the process. Inspect the label edges and
their rationale. The source remains the literal R1 observation. A shared word
in another about would need its own source-backed identity declaration.

```json
{
  "tool": "kmp_view_open",
  "save_as": "view",
  "arguments": {
    "about": "example:guide:dimensional-memberships"
  }
}
```

```json
{
  "tool": "kmp_view_apply_intent",
  "save_as": "frame",
  "arguments": {
    "view_id": "${view.view_id}",
    "expected_revision": "${view.view_revision}",
    "idempotency_key": "guide-dimensional:view",
    "selection": "${original.generated_refs.0}",
    "focus": {
      "time_range": {
        "from": "2026-09-01T09:55:00Z",
        "to": "2026-09-02T10:05:00Z",
        "axis": "observed"
      }
    },
    "projection": {
      "semantic_zoom": "moment"
    },
    "explanation": "Review the original source, independent label facets and late memberships"
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
