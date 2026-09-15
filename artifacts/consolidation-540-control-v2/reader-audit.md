# Reader semantic audit — development control v2

All 96 answers are supported by their supplied packets. Required original-fact recovery is complete for 86/96: 62/72 answerable questions plus all 24 expected Q7/Q8 abstentions. Four answers are partial and six correctly abstain because necessary packet evidence is absent. No unavailable citations, false identity merges, inferred independent corroboration, invented signature dates, or permission inferred from ownership were found.

**Original-text caveat:** the frozen Nora sources say “for staging”; their fixed declarations add “staging only”. Five consolidated-reader answers repeat explicit exclusivity (Q3 at all budgets, Q7 at 100/75). Thus strict original-text support is 91/96, while packet support remains 96/96. These five are a writer-declaration error, not a reader hallucination. Required fact recovery and this extra qualifier are assessed separately.

| Arm | Budget | Packet supported | Original supported | Complete recovery | Partial | Correct no-evidence abstention | Expected abstention | Bad refs |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| source-only | 100% | 8/8 | 8/8 | 8/8 | 0 | 0 | 2 | 0 |
| source-only | 75% | 8/8 | 8/8 | 7/8 | 1 | 0 | 2 | 0 |
| source-only | 50% | 8/8 | 8/8 | 6/8 | 1 | 1 | 2 | 0 |
| formed | 100% | 8/8 | 8/8 | 8/8 | 0 | 0 | 2 | 0 |
| formed | 75% | 8/8 | 8/8 | 7/8 | 1 | 0 | 2 | 0 |
| formed | 50% | 8/8 | 8/8 | 6/8 | 1 | 1 | 2 | 0 |
| deduplicated | 100% | 8/8 | 8/8 | 8/8 | 0 | 0 | 2 | 0 |
| deduplicated | 75% | 8/8 | 8/8 | 7/8 | 0 | 1 | 2 | 0 |
| deduplicated | 50% | 8/8 | 8/8 | 5/8 | 0 | 3 | 2 | 0 |
| consolidated | 100% | 8/8 | 6/8 | 8/8 | 0 | 0 | 2 | 0 |
| consolidated | 75% | 8/8 | 6/8 | 8/8 | 0 | 0 | 2 | 0 |
| consolidated | 50% | 8/8 | 7/8 | 8/8 | 0 | 0 | 2 | 0 |

Complete recovery includes the two expected abstentions per run. “Correct no-evidence abstention” applies only to Q1–Q6 where originals answer the question but the budgeted packet does not. Partial and no-evidence categories are recovery losses, not unsupported-answer errors.

## Notable recovery losses

- Source-only and formed at 75%: Q5 retains tentative permission awaiting signature but cannot recover the omitted unresolved denial. Both acknowledge the missing conflict evidence.
- Source-only and formed at 50%: Q4 retains R7’s publication denial but cannot recover the omitted copy-only checksum evidence; Q5 appropriately abstains because all R8 evidence is absent.
- Deduplicated at 75%: Q5 appropriately abstains because all R8 evidence is absent.
- Deduplicated at 50%: Q3, Q4, and Q5 appropriately abstain because October ownership, R7, and R8 evidence are absent. September ownership is not carried into October.
- Consolidated at all budgets: all required answer facts remain available and recovered. The inherited Nora exclusivity caveat above prevents an unqualified claim of perfect original-source entailment.

## Evidence and method

The audit read the frozen 16 original records, all fixed declarations, eight questions, twelve context packets, and twelve reader outputs. Each of the 96 answer judgments and every citation resolution appears in `reader-audit.json`; input SHA-256 hashes identify the exact reviewed artifacts. Omitted read instructions never count as source content. Source refs on visible formed records and consolidated declarations resolve to their visible evidence; this does not pretend the full original is present.

Deduplicated-50 legitimately cites source:1:0 via visible formed:2. Empty source_refs on consolidated Q6 at 75/50 are supported by the packet’s explicit semantic_validation metadata. Q7 and Q8 are correctly treated as expected abstentions throughout; ownership is not production permission, and no exact R8 signature date appears anywhere.

This single hand-authored development control tests these fixed statements and budget selections. It is not a general semantic-quality benchmark or evidence of general superiority across workloads.
