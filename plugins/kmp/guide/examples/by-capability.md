# Find a concrete example by capability

Start with [one decision and its reason](./first-decision.md). It has two
source lines and a direct write → inspect → retrieve → audit path. The other
basic files group independent small cases so each relation has its own source,
rationale and limit. Read the source and prerequisite writes before a selected
call; a binding to an earlier result is not a ref you can invent.

This index was reviewed against the development tools/list on September 8,
2026: fifteen tools, ten memory kinds and thirty-one writer relations. It is
an informative map, not a CI gate or a promise that another LLM has learned
these choices. Runtime names and allowed classes come from tools/list.

A `save_as` name identifies the exact JSON call inside its lesson. In KMP,
inspect the corresponding guide ref from the table at the end, using about
`guide:kmp-agent`. The lesson contains the literal sources, valid arguments,
returned-ref bindings, expected evidence and negative limits.

## Tools

| Tool | Concrete call | Limit to read with it |
| --- | --- | --- |
| `kmp_ingest` | [canonical-ingest](./canonical-ingest.md), `ingested` | The referenced constraint must already exist; preview alone writes nothing. |
| `kmp_write_memory` | [first-decision](./first-decision.md), `constraint` | A recorded choice is not a passed test. |
| `kmp_wake` | [preference-delta](./preference-delta.md), `present` | A compact catalogue does not include every expanded proof item. |
| `kmp_ask` | [first-decision](./first-decision.md), `answer` | For absent evidence, use the terminal UNKNOWN case in budget-proof. |
| `kmp_relate` | [distributed-incident](./distributed-incident.md), `related` | A proposal does not write or prove an equivalence. |
| `kmp_goto` | [decision-history](./decision-history.md), `start` | Keep the requested clock and the inclusive boundary. |
| `kmp_near` | [alias-ownership](./alias-ownership.md), `account_history` | Copy the returned ref; nearby mentions are not proven identities. |
| `kmp_rewind` | [decision-history](./decision-history.md), `earlier` | An older decision remains historical after replacement. |
| `kmp_forward` | [budget-proof](./budget-proof.md), `temporal_first` | Continue temporal_second; exclude the interval end. |
| `kmp_trace` | [first-decision](./first-decision.md), `packet_proof` | For an absent path, use no_path in budget-proof. |
| `kmp_inspect` | [first-decision](./first-decision.md), `constraint_read` | If a page is partial, finish it before claiming full evidence. |
| `kmp_relabel` | [labels-negation](./labels-negation.md), `canonical_label` | Rename a label without inventing identity or replacing the source. |
| `kmp_view_open` | [first-decision](./first-decision.md), `view` | The view is process-scoped; memory is durable. |
| `kmp_view_apply_intent` | [first-decision](./first-decision.md), `frame` | After a revision conflict, read the human frame before another move. |
| `kmp_view_get_state` | [shared-resumption](./shared-resumption.md), `human_state` | A screen gesture is not authorization to write feedback. |

## Memory kinds

Intent names the writer operation; kind names what is remembered. In the
record_delta case, the writer also generates a separate semantic_delta entry.

| Kind | Source and call | What it records |
| --- | --- | --- |
| `turn` | [shared-resumption](./shared-resumption.md), `handoff` | H1: Please confirm D1 is the selection to review for HND-9. The offline restore test is still pending; no test report is available. |
| `observation` | [workflow-proof](./workflow-proof.md), `slow` | B1: LAT-3 build A measured 150 ms for the response under the R1 test conditions. |
| `decision` | [first-decision](./first-decision.md), `decision` | D1: START-1 chooses SQLite because C1 requires journal writes without a network. |
| `feedback` | [quantities](./quantities.md), `void` | TRIP-7 transaction TX-MEAL from M-1 was voided in full. Its posted 10.00 EUR leaves 0.00 EUR settled. |
| `semantic_delta` | [preference-delta](./preference-delta.md), `new_policy` | D2: POL-2 replaces D1 for normal NOTIFY-2 events: use immediate notices because the operations team requested immediate updates. Urgent alerts remain immediate. |
| `constraint` | [first-decision](./first-decision.md), `constraint` | C1: START-1 requires the journal to accept writes without a network. |
| `preference` | [preference-delta](./preference-delta.md), `preference` | P1: For NOTIFY-2, I prefer one daily digest of normal events. Urgent alerts may interrupt; this is a preference, not a mandatory limit. |
| `derived_value` | [structure-parts](./structure-parts.md), `total` | TOTAL1: The checked CLAIM-4 settlement totals 55 EUR: bus receipt BUS1 is 20 EUR and hotel receipt HOTEL1 is 35 EUR. These are two distinct receipts; no other charges are included. |
| `error_path` | [distributed-incident](./distributed-incident.md), `audit` | The deployment of rel-17 exhausted Atlas database connections at 09:00 UTC on September 1: outage EVT-17-F in INC-17. The audit compared it with recovery EVT-17-R. |
| `success_path` | [workflow-proof](./workflow-proof.md), `execution` | EX1: RLS-3 staging deployment OP1 completed after its checksum check. The execution log names A1 as the approval required before this deployment could run. |

## Writer relations

The selected class is the one justified by that source. The allowed column
shows the current alternatives; it does not tell the writer to emit all of them.
Structural links below connect source-declared user memories. Internal records,
contains_entry and has_dimension links are created by KMP and are not copied.

| Relation | Concrete source call | Class used; allowed classes | Rationale to check against the source |
| --- | --- | --- | --- |
| `follows` | [labels-negation](./labels-negation.md), `later_decision` | `procedural`; `procedural` | La decisión D1 se registra después de revisar los informes de RUN-7; este enlace conserva el orden y no verifica ni sustituye sus estados anteriores. |
| `answers` | [shared-resumption](./shared-resumption.md), `feedback` | `evidential`; `evidential` | F1 explicitly answers H1, the stored request for confirmation of the review selection. |
| `uses_background` | [structure-parts](./structure-parts.md), `total` | `evidential`; `evidential` | TOTAL1 settles the two components described by BREAKDOWN1; the literal settlement supplies amounts and exclusions. |
| `depends_on` | [workflow-proof](./workflow-proof.md), `execution` | `causal`; `causal` | EX1 records A1 as the required approval for this completed staging deployment. |
| `chosen_because` | [first-decision](./first-decision.md), `decision` | `motivational`; `causal`, `motivational` | D1 explicitly chooses SQLite for the offline-write requirement recorded by C1. |
| `triggers` | [workflow-proof](./workflow-proof.md), `sample` | `causal`; `causal` | The S1 rule log identifies this threshold event as the cause of AL1, not merely an earlier sample. |
| `authorizes` | [workflow-proof](./workflow-proof.md), `approval` | `motivational`; `causal`, `motivational` | A1 explicitly permits OP1 in staging, with checksum verification required and production excluded. |
| `verified_by` | [budget-proof](./budget-proof.md), `result` | `evidential`; `evidential` | R1's matching-checksum and offline-completion claims are verified by the measured result recorded in T1. |
| `semantic_delta_from` | [preference-delta](./preference-delta.md), `new_policy` (generated delta link) | `causal`; `causal` | D2 replaces the default because operations requested immediate updates; the urgent-alert policy is unchanged. |
| `updates_state` | [preference-delta](./preference-delta.md), `new_policy` (generated delta link) | `causal`; `causal` | D2 replaces the default because operations requested immediate updates; the urgent-alert policy is unchanged. |
| `supports` | [quantities](./quantities.md), `statement` | `evidential`; `evidential` | The closing statement independently confirms the corrected settled hotel figure; it does not create another charge. |
| `supersedes` | [preference-delta](./preference-delta.md), `new_policy` | `evidential`; `evidential` | D2 explicitly replaces D1 for normal NOTIFY-2 events while retaining the urgent-alert exception. |
| `contradicts` | [labels-negation](./labels-negation.md), `negative` | `evidential`; `evidential` | S1 afirma habilitado y S3 lo niega para el mismo RUN-7, prod, Ana e instante; G1 prueba que bitácora y journal designan el mismo componente en este catálogo. |
| `satisfies_constraint` | [workflow-proof](./workflow-proof.md), `fast` | `constraint`; `constraint` | G1 measures 80 ms under R1 conditions, within the required maximum of 100 ms. |
| `violates_constraint` | [workflow-proof](./workflow-proof.md), `slow` | `constraint`; `constraint` | B1 measures 150 ms under R1 conditions, exceeding the required maximum of 100 ms. |
| `contributes_to` | [structure-parts](./structure-parts.md), `bus` | `evidential`; `evidential` | BUS1 is explicitly included as 20 EUR in TOTAL1, whose source also identifies HOTEL1 at 35 EUR. |
| `excluded_from` | [quantities](./quantities.md), `exclusions` | `constraint`; `constraint` | These documented candidates are deliberately omitted from this EUR subtotal for replacement, duplicate, void or unit mismatch reasons. |
| `checked_against` | [quantities](./quantities.md), `total` | `constraint`; `constraint` | The calculation is checked against the rule requiring unique settled EUR charges and no voids or currency conversion. |
| `derived_from` | [quantities](./quantities.md), `exclusions` | `evidential`; `evidential` | This is the preliminary operand omitted after its explicit correction. |
| `confirms_selection` | [shared-resumption](./shared-resumption.md), `feedback` | `evidential`; `evidential`, `motivational` | F1 explicitly confirms D1 as the selection to review. It limits that confirmation to selection and leaves the offline test pending. |
| `restates` | [labels-negation](./labels-negation.md), `restated` | `evidential`; `evidential` | S2 declara que reformula S1: ambos afirman habilitado para RUN-7, prod, Ana y el mismo instante; G1 identifica los dos nombres del componente. |
| `corrects` | [quantities](./quantities.md), `hotel_final` | `evidential`; `evidential` | The final invoice corrects the amount of this same hotel transaction; it must replace, not be added to, the preliminary figure. |
| `component_of` | [structure-parts](./structure-parts.md), `bus` | `evidential`; `evidential` | BUS1 is the transport component explicitly listed in BREAKDOWN1. |
| `total_of` | [quantities](./quantities.md), `total` | `evidential`; `evidential` | The subtotal includes this distinct settled transport charge exactly once. |
| `same_event_as` | [quantities](./quantities.md), `hotel_copy` | `evidential`; `evidential` | The copy and final invoice report one hotel charge; the explicit transaction and copy statement justify counting it once. |
| `same_entity_as` | [alias-ownership](./alias-ownership.md), `alias` | `evidential`; `evidential` | La aclaración firmada identifica expresamente al sujeto llamado Nora con Elena Vega, la persona nombrada en el directorio. |
| `qualifies_as` | [structure-parts](./structure-parts.md), `bus` | `evidential`; `evidential` | BUS1 meets the CATEGORY1 criteria through its identified receipt, amount, public transport and work purpose. |
| `matches_requirement` | [workflow-proof](./workflow-proof.md), `candidate` | `constraint`; `constraint` | C1 has both properties required by R2: local checksum verification and operation without a network. |
| `contains` | [structure-parts](./structure-parts.md), `plan` | `structural`; `structural` | PLAN1 explicitly lists PH1 as a phase of PLAN-4; this records organization only. |
| `member_of` | [structure-parts](./structure-parts.md), `member` | `structural`; `structural` | MEMBER1 names Ivo in the REVIEW-4 roster and states no approval rights. |
| `scoped_to` | [structure-parts](./structure-parts.md), `rule` | `structural`; `structural` | RULE1 limits its 30-day retention requirement to STAGE-4 and excludes a claim about production. |

## Exact KMP lesson references

These refs are copied from the generated guide requests. Load one selected
lesson, reuse it within the session, and read its prerequisites as needed.

| Lesson | Ref in guide:kmp-agent |
| --- | --- |
| [first-decision](./first-decision.md) | `guide:kmp-agent:example:first-decision` |
| [preference-delta](./preference-delta.md) | `guide:kmp-agent:example:preference-delta` |
| [workflow-proof](./workflow-proof.md) | `guide:kmp-agent:example:workflow-proof` |
| [structure-parts](./structure-parts.md) | `guide:kmp-agent:example:structure-parts` |
| [canonical-ingest](./canonical-ingest.md) | `guide:kmp-agent:example:canonical-ingest` |
| [budget-proof](./budget-proof.md) | `guide:kmp-agent:example:budget-proof` |
| [quantities](./quantities.md) | `guide:kmp-agent:example:quantities` |
| [shared-resumption](./shared-resumption.md) | `guide:kmp-agent:example:shared-resumption` |
| [labels-negation](./labels-negation.md) | `guide:kmp-agent:example:labels-negation` |
| [decision-history](./decision-history.md) | `guide:kmp-agent:example:decision-history` |
| [alias-ownership](./alias-ownership.md) | `guide:kmp-agent:example:alias-ownership` |
| [distributed-incident](./distributed-incident.md) | `guide:kmp-agent:example:distributed-incident` |
| [four-clocks](./four-clocks.md) | `guide:kmp-agent:example:four-clocks` |
| [late-conflict](./late-conflict.md) | `guide:kmp-agent:example:late-conflict` |

## What this review proves

The linked calls exercise all the named capabilities with explicit source
interpretations. The standalone replays check kinds, refs, clocks and proof
preservation; the visual reviews check their presentation. Counting a name
in a file would not prove any of those properties. Independent learning on
new sources and reader questions remains a separate evaluation.

The multivalue contract is demonstrated in [dimensional-memberships](./dimensional-memberships.md), exact ref `guide:kmp-agent:example:dimensional-memberships`: arrays, independent keys, selectors, late relabel and visual review.
