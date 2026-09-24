## Per-increment history

Earlier paired runs, each against the integration head before the increment (details in
`agent-token-optimization.md`). Figures are whole-journey weighted changes, `o200k_base`,
`json_compact_lexical_v1`; A/A controls were zero in every run.

| Increment | PR | Baseline | 4096 B | 10000 B | Oracle |
|---|---|---|---:|---:|---|
| I1–I2b + I3 (Wake evidence, state, scope) | #835 #837 #840 #839 | main a22b6402 | −6.9 % | −1.0 % | candidate 11/11, baseline 3/11 |
| C1 catalogue without output schemas | #841 | — (fixture) | default `tools/list` 51,629 → 29,460 tokens | | unchanged |
| PR 3 incremental continuation pages | #842 | d339c02f | −3.4 % | −1.2 % | 11/11 both |
| C2 one time-navigation verb | #843 | — (fixture) | default `tools/list` 29,506 → 24,899 tokens | | unchanged |
| C3 minimal continuation actions, lean progress | #845 | dcb8b5d8 | −1.3 % | −0.85 % | 11/11 both |

The I3 run of 2026-09-24 remains in `artifacts/544-wake-oracle-20260924/`.
