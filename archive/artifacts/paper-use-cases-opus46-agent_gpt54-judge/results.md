# Paper Use Case Results

Source: `artifacts/paper-use-cases/summary.json`

## Metrics Table

| Use case | Variant | Relation mode | Detail mode | Scale | Budget | Explanation fidelity | Detail fidelity | Causal score | Retry hit | Retry score | Tokens |
| --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| uc1_failure_diagnosis_rehydration | detail_only_with_detail | detail_only | with_detail | micro | 4096 | 0.0 | 1.0 | 0.42857142857142855 |  | 0.0 | 245 |
| uc1_failure_diagnosis_rehydration | full_explanatory_with_detail | explanatory | with_detail | micro | 4096 | 1.0 | 1.0 | 1.0 | true | 1.0 | 282 |
| uc1_failure_diagnosis_rehydration | full_explanatory_with_detail__meso | explanatory | with_detail | meso | 4096 | 1.0 | 1.0 | 1.0 | true | 1.0 | 282 |
| uc1_failure_diagnosis_rehydration | full_explanatory_without_detail | explanatory | without_detail | micro | 4096 | 1.0 | 0.0 | 0.8571428571428571 | true | 1.0 | 252 |
| uc1_failure_diagnosis_rehydration | structural_only_with_detail | structural_only | with_detail | micro | 4096 | 0.0 | 1.0 | 0.14285714285714285 |  | 0.0 | 200 |
| uc2_why_implementation_trace | detail_only_with_detail | detail_only | with_detail | micro | 4096 | 0.0 | 1.0 | 0.42857142857142855 |  |  | 247 |
| uc2_why_implementation_trace | full_explanatory_with_detail | explanatory | with_detail | micro | 4096 | 1.0 | 1.0 | 1.0 |  |  | 285 |
| uc2_why_implementation_trace | full_explanatory_with_detail__meso | explanatory | with_detail | meso | 4096 | 1.0 | 1.0 | 1.0 |  |  | 285 |
| uc2_why_implementation_trace | full_explanatory_without_detail | explanatory | without_detail | micro | 4096 | 1.0 | 0.0 | 0.8571428571428571 |  |  | 255 |
| uc2_why_implementation_trace | structural_only_with_detail | structural_only | with_detail | micro | 4096 | 0.0 | 1.0 | 0.14285714285714285 |  |  | 203 |
| uc3_interrupted_handoff_resume | detail_only_with_detail | detail_only | with_detail | micro | 4096 | 0.0 | 1.0 | 0.42857142857142855 |  | 0.0 | 441 |
| uc3_interrupted_handoff_resume | full_explanatory_with_detail | explanatory | with_detail | micro | 4096 | 1.0 | 1.0 | 1.0 | true | 1.0 | 575 |
| uc3_interrupted_handoff_resume | full_explanatory_with_detail__meso | explanatory | with_detail | meso | 4096 | 1.0 | 1.0 | 1.0 | true | 1.0 | 575 |
| uc3_interrupted_handoff_resume | full_explanatory_without_detail | explanatory | without_detail | micro | 4096 | 1.0 | 0.0 | 0.8571428571428571 | true | 1.0 | 535 |
| uc3_interrupted_handoff_resume | structural_only_with_detail | structural_only | with_detail | micro | 4096 | 0.0 | 1.0 | 0.14285714285714285 |  | 0.0 | 374 |
| uc4_constraint_reason_under_token_pressure | full_explanatory_with_detail | explanatory | with_detail | micro | 4096 | 1.0 | 1.0 | 1.0 |  |  | 223 |
| uc4_constraint_reason_under_token_pressure | full_explanatory_with_detail__budget_192 | explanatory | with_detail | micro | 192 | 1.0 | 0.0 | 1.0 |  |  | 175 |
| uc4_constraint_reason_under_token_pressure | full_explanatory_with_detail__meso | explanatory | with_detail | meso | 4096 | 1.0 | 1.0 | 1.0 |  |  | 223 |
| uc4_constraint_reason_under_token_pressure | structural_only_with_detail__budget_96 | structural_only | with_detail | micro | 96 | 0.0 | 0.0 | 0.125 |  |  | 73 |

## Key Findings

- uc1_failure_diagnosis_rehydration: `detail_only_with_detail` reaches explanation fidelity `0.0`, causal score `0.42857142857142855`, retry score `0.0`, and renders `245` tokens under budget `4096`.
- uc1_failure_diagnosis_rehydration: `full_explanatory_with_detail` reaches explanation fidelity `1.0`, causal score `1.0`, retry score `1.0`, and renders `282` tokens under budget `4096`.
- uc1_failure_diagnosis_rehydration: `full_explanatory_with_detail__meso` reaches explanation fidelity `1.0`, causal score `1.0`, retry score `1.0`, and renders `282` tokens under budget `4096`.
- uc1_failure_diagnosis_rehydration: `full_explanatory_without_detail` reaches explanation fidelity `1.0`, causal score `0.8571428571428571`, retry score `1.0`, and renders `252` tokens under budget `4096`.
- uc1_failure_diagnosis_rehydration: `structural_only_with_detail` reaches explanation fidelity `0.0`, causal score `0.14285714285714285`, retry score `0.0`, and renders `200` tokens under budget `4096`.
- uc2_why_implementation_trace: `detail_only_with_detail` reaches explanation fidelity `0.0`, causal score `0.42857142857142855`, retry score `n/a`, and renders `247` tokens under budget `4096`.
- uc2_why_implementation_trace: `full_explanatory_with_detail` reaches explanation fidelity `1.0`, causal score `1.0`, retry score `n/a`, and renders `285` tokens under budget `4096`.
- uc2_why_implementation_trace: `full_explanatory_with_detail__meso` reaches explanation fidelity `1.0`, causal score `1.0`, retry score `n/a`, and renders `285` tokens under budget `4096`.
- uc2_why_implementation_trace: `full_explanatory_without_detail` reaches explanation fidelity `1.0`, causal score `0.8571428571428571`, retry score `n/a`, and renders `255` tokens under budget `4096`.
- uc2_why_implementation_trace: `structural_only_with_detail` reaches explanation fidelity `0.0`, causal score `0.14285714285714285`, retry score `n/a`, and renders `203` tokens under budget `4096`.
- uc3_interrupted_handoff_resume: `detail_only_with_detail` reaches explanation fidelity `0.0`, causal score `0.42857142857142855`, retry score `0.0`, and renders `441` tokens under budget `4096`.
- uc3_interrupted_handoff_resume: `full_explanatory_with_detail` reaches explanation fidelity `1.0`, causal score `1.0`, retry score `1.0`, and renders `575` tokens under budget `4096`.
- uc3_interrupted_handoff_resume: `full_explanatory_with_detail__meso` reaches explanation fidelity `1.0`, causal score `1.0`, retry score `1.0`, and renders `575` tokens under budget `4096`.
- uc3_interrupted_handoff_resume: `full_explanatory_without_detail` reaches explanation fidelity `1.0`, causal score `0.8571428571428571`, retry score `1.0`, and renders `535` tokens under budget `4096`.
- uc3_interrupted_handoff_resume: `structural_only_with_detail` reaches explanation fidelity `0.0`, causal score `0.14285714285714285`, retry score `0.0`, and renders `374` tokens under budget `4096`.
- uc4_constraint_reason_under_token_pressure: `full_explanatory_with_detail` reaches explanation fidelity `1.0`, causal score `1.0`, retry score `n/a`, and renders `223` tokens under budget `4096`.
- uc4_constraint_reason_under_token_pressure: `full_explanatory_with_detail__budget_192` reaches explanation fidelity `1.0`, causal score `1.0`, retry score `n/a`, and renders `175` tokens under budget `192`.
- uc4_constraint_reason_under_token_pressure: `full_explanatory_with_detail__meso` reaches explanation fidelity `1.0`, causal score `1.0`, retry score `n/a`, and renders `223` tokens under budget `4096`.
- uc4_constraint_reason_under_token_pressure: `structural_only_with_detail__budget_96` reaches explanation fidelity `0.0`, causal score `0.125`, retry score `n/a`, and renders `73` tokens under budget `96`.
