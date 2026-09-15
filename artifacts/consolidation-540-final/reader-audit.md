# Strict reader audit — corrected control v3

All 96 answers are supported by their packets and by the original source prose under the unchanged v2 rubric. Complete original-fact recovery is 86/96: 62/72 answerable questions plus all 24 expected Q7/Q8 abstentions. Four responses are partial; six appropriately abstain because required packet evidence is absent. There are no invalid citations, false identity merges, production-permission inferences from ownership, independent-corroboration inventions, or signature-date inventions.

| Arm | Budget (bytes) | Strict support | Complete recovery | Partial | Correct missing-evidence abstention | Expected abstention | Temporal / negative omissions* | Bad refs |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| source-only | 100% (13791) | 8/8 | 8/8 | 0 | 0 | 2 | 0 / 0 | 0 |
| source-only | 75% (10343) | 8/8 | 7/8 | 1 | 0 | 2 | 0 / 1 | 0 |
| source-only | 50% (6895) | 8/8 | 6/8 | 1 | 1 | 2 | 0 / 1 | 0 |
| formed | 100% (13791) | 8/8 | 8/8 | 0 | 0 | 2 | 0 / 0 | 0 |
| formed | 75% (10343) | 8/8 | 7/8 | 1 | 0 | 2 | 0 / 1 | 0 |
| formed | 50% (6895) | 8/8 | 6/8 | 1 | 1 | 2 | 0 / 1 | 0 |
| deduplicated | 100% (13791) | 8/8 | 8/8 | 0 | 0 | 2 | 0 / 0 | 0 |
| deduplicated | 75% (10343) | 8/8 | 7/8 | 0 | 1 | 2 | 0 / 1 | 0 |
| deduplicated | 50% (6895) | 8/8 | 5/8 | 0 | 3 | 2 | 1 / 3 | 0 |
| consolidated | 100% (13791) | 8/8 | 8/8 | 0 | 0 | 2 | 0 / 0 | 0 |
| consolidated | 75% (10343) | 8/8 | 8/8 | 0 | 0 | 2 | 0 / 0 | 0 |
| consolidated | 50% (6895) | 8/8 | 8/8 | 0 | 0 | 2 | 0 / 0 | 0 |

*Temporal/negative columns count answer-level recovery omissions caused by missing packet evidence, and overlap the partial/abstention columns. No reader drops a temporal or negative qualifier that is visible in its supplied packet. Complete recovery includes expected Q7/Q8 abstentions; correct missing-evidence abstentions on Q1–Q6 remain incomplete recovery.

Source-only/formed at 75% lose the unresolved R8 denial in Q5 while correctly retaining tentative permission pending signature. At 50%, these arms also lose R7’s copy-only checksum qualification in Q4 and all R8 evidence in Q5. Deduplicated75 loses all R8 evidence for Q5. Deduplicated50 lacks October ownership, R7, and R8 evidence for Q3–Q5 and correctly abstains. This amounts to one temporal, eight negative/conflict, and three copy-only answer-level recovery omissions; categories overlap.

## Failed writer proposal and corrected follow-up

V2 remains a failed writer proposal under strict original-text entailment: the Nora sources said “for staging”, while the fixed declarations added “staging only”. Five consolidated answers inherited the added exclusivity. V2 remains 91/96 on strict source support, with its artifacts and audit unchanged.

V3 changes Nora’s fixed declaration qualifier to “staging”. The 16 original source records and eight questions are unchanged. All three new consolidated reader outputs preserve the original scope, the October ownership transition, and the unresolved R8 evidence. They recover all eight answers at each budget. This corrected follow-up does not retroactively validate the first proposal.

## Audit evidence and limits

The nine reused source-only, formed, and deduplicated packet/reader pairs were independently compared byte for byte with v2, as were the questions. All matched. Their existing strict judgments therefore remain applicable. All 24 new consolidated answers and corrected claims were read and assessed anew. The JSON audit contains all 96 judgments, current citation resolutions, hashes, omission explanations, and reuse-verification results.

Every cited ref is available through a visible record, passage mapping, formed-record source_ref, or consolidated declaration. Omitted read instructions are not evidence. Deduplicated50’s source:1:0 citations resolve through visible formed:2. Empty consolidated Q6 refs rely on explicit validation metadata, and empty Q7 refs accompany correct packet-level abstentions; neither fabricates a citation.

Shared absolute budgets are 13,791 / 10,343 / 6,895 bytes, based on the full formed JSON envelope. The measurements file reports whole-CLI costs separately. This hand-authored development control is not a generalized benchmark, proof of semantic kernel verification, or a warmed-service/billed-cost study.
