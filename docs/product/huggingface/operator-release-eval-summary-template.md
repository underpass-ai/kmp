# Operator Release Evaluation Summary Template

Use this file to create the public evaluation summary for a specific
`underpass-ai/kernel-tool-operator-small` release.

Do not publish a release summary until every required result is filled from a
fresh run.

## Release

| Item | Value |
| --- | --- |
| Model repo | `underpass-ai/kernel-tool-operator-small` |
| Dataset repo | `underpass-ai/kernel-operator-trajectories` |
| Model version/tag | `<fill>` |
| Dataset version/tag | `<fill>` |
| Kernel commit | `<fill>` |
| Run id | `<fill>` |
| Date | `<fill>` |

## Data Generation

| Item | Value |
| --- | --- |
| Benchmark/source | `<fill>` |
| Tasks | `<fill>` |
| Replay events | `<fill>` |
| Smart-writer mode | `<fill>` |
| MCP navigation probe | `<fill>` |
| Public endpoint used | `<fill>` |
| Fresh run_id/about namespace | `<fill>` |
| Future leaks | `<fill>` |

## Split

| Item | Value |
| --- | --- |
| Split mode | grouped |
| Group key | `<fill>` |
| Train rows | `<fill>` |
| Eval rows | `<fill>` |
| Dropped non-visible target refs | `<fill>` |
| Synthetic refs | yes |
| Leak audit | `<fill>` |

## Offline Policy Evaluation

| Predictor | Total | Exact | Tool | Ref | Scope | Stop | Invalid | Unbounded |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Deterministic baseline | `<fill>` | `<fill>` | `<fill>` | `<fill>` | `<fill>` | `<fill>` | `<fill>` | `<fill>` |
| Generalist LLM baseline | `<fill>` | `<fill>` | `<fill>` | `<fill>` | `<fill>` | `<fill>` | `<fill>` | `<fill>` |
| Operator model | `<fill>` | `<fill>` | `<fill>` | `<fill>` | `<fill>` | `<fill>` | `<fill>` | `<fill>` |

## De-Anonymization

| Item | Value |
| --- | ---: |
| Selected predictions | `<fill>` |
| Written raw predictions | `<fill>` |
| Failures | `<fill>` |
| Mapped synthetic refs | `<fill>` |

## Live MCP Replay

| Item | Value |
| --- | ---: |
| Selected trajectory steps | `<fill>` |
| Executed tool calls | `<fill>` |
| Stop actions | `<fill>` |
| Successful tool calls | `<fill>` |
| Failed tool calls | `<fill>` |
| Missing expected ref rows | `<fill>` |
| Missing predictions | `<fill>` |
| Invalid predictions | `<fill>` |
| Unbounded tool calls | `<fill>` |
| Extra observed ref rows | `<fill>` |
| Elapsed | `<fill>` |

## Live Replay Latency

| Action | Count | Avg ms | Max ms |
| --- | ---: | ---: | ---: |
| `kmp_near` | `<fill>` | `<fill>` | `<fill>` |
| `kmp_inspect` | `<fill>` | `<fill>` | `<fill>` |
| `kmp_trace` | `<fill>` | `<fill>` | `<fill>` |
| `kmp_goto` | `<fill>` | `<fill>` | `<fill>` |
| `kmp_rewind` | `<fill>` | `<fill>` | `<fill>` |
| `kmp_forward` | `<fill>` | `<fill>` | `<fill>` |
| `kmp_ask` | `<fill>` | `<fill>` | `<fill>` |
| `stop` | `<fill>` | `<fill>` | `<fill>` |

## Release Decision

Publish only if all rows below are green.

| Gate | Result |
| --- | --- |
| Fresh audited run larger than current 100-task corpus | `<fill>` |
| Grouped split | `<fill>` |
| Prompt leak audit clean | `<fill>` |
| Zero invalid predictions | `<fill>` |
| Zero unbounded calls | `<fill>` |
| Zero live MCP/gRPC failures | `<fill>` |
| Zero missing expected refs in replay | `<fill>` |
| Dataset/model cards complete | `<fill>` |

## Notes

Add any limitation or surprising result here. Do not hide failures behind
aggregate accuracy.
