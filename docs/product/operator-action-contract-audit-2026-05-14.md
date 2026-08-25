# Operator Action Contract Audit - 2026-05-14

Esta nota documenta el impacto del problema detectado en el pipeline del
Operator: el evaluador y el predictor aceptaban acciones que se parecian al
contrato KMP/MCP, pero que no cumplian exactamente el schema.

La conclusion corta es:

```text
No es un bug del kernel.
Es un bug de medicion y validacion en el pipeline del modelo Operator.
```

El impacto es importante para claims de modelo, benchmarks del Operator y
publicacion en Hugging Face. No invalida KMP core, MCP/gRPC, persistencia,
temporalidad, multidimensionalidad ni los grafos escritos en el kernel.

## Decision

A partir de este corte, ninguna metrica del Operator debe considerarse
publicable si no pasa por validacion estricta del contrato de accion.

Las metricas anteriores deben etiquetarse como `pre-strict` salvo que se hayan
reejecutado con:

- predictor que rechaza campos fuera de schema;
- evaluador que usa el mismo contrato estricto;
- cero acciones invalidas;
- cero llamadas no acotadas;
- replay MCP/gRPC real solo despues de validar el contrato.

No se debe aplicar ningun fallback ni limpieza silenciosa de acciones. Si una
accion no cumple el contrato, falla y queda registrada como invalida.

## Que Paso

El Operator aprende a emitir una unica accion KMP/MCP desde estado visible de
memoria:

```json
{
  "action": {
    "type": "tool_call",
    "tool": "kernel_ask",
    "arguments": {
      "about": "ref_0001",
      "answer_policy": "evidence_or_unknown",
      "dimensions": {
        "mode": "all",
        "scope": "current_about"
      },
      "question": "Which book did I finish reading first?"
    }
  }
}
```

El contrato real no permite campos arbitrarios dentro de `arguments`. Sin
embargo, el pipeline anterior aceptaba una accion como esta:

```json
{
  "action": {
    "type": "tool_call",
    "tool": "kernel_ask",
    "arguments": {
      "about": "ref_0001",
      "answer_policy": "evidence_or_unknown",
      "dimensions": {
        "mode": "all",
        "scope": "current_about"
      },
      "question": "Which book did I finish reading first?",
      "final_refs": ["ref_0002"]
    }
  }
}
```

Ese `final_refs` dentro de `kernel_ask.arguments` es invalido. `final_refs`
solo pertenece a una accion terminal `stop`, no a una llamada de herramienta
`kernel_ask`.

El problema no era que el modelo eligiera necesariamente mal la herramienta. El
problema era que el pipeline podia registrar como prediccion valida una accion
fuera del contrato.

## Suposicion Incorrecta

La validacion anterior confundia tres niveles distintos:

| Nivel | Pregunta correcta | Estado anterior |
| --- | --- | --- |
| JSON parseable | Se puede parsear como JSON? | Si se comprobaba. |
| Accion basica | Tiene `type`, `tool` y `arguments`? | Si se comprobaba parcialmente. |
| Contrato KMP/MCP exacto | Usa solo campos permitidos, tipos correctos, limites y cursores validos? | No se comprobaba completo. |

El error estaba en tratar "parecido a una accion" como "accion valida".

Eso es inaceptable para Operator porque su producto no es texto libre. Su
producto es una decision operacional que debe poder ejecutarse contra KMP/MCP.

## Componentes Afectados

| Componente | Afectado | Motivo |
| --- | --- | --- |
| `scripts/operator/predict_operator_sft.py` | Si | El predictor aceptaba una accion si tenia forma basica, aunque los argumentos incluyeran campos extra. |
| `kernel_operator_policy_eval` | Si | El evaluador contaba como valida una `tool_call` si tenia `tool` y `arguments`. |
| `kernel_operator_llm_baseline` | Si | La baseline LLM compartia una validacion demasiado permisiva. |
| Metricas historicas del Operator | Si, para claims | Las metricas de validez, tool/ref/scope y replay deben revalidarse si se quieren publicar. |
| Entrenamiento SFT | No directamente | Los targets de entrenamiento pueden seguir siendo estrictos; el bug estaba en aceptar outputs incorrectos al evaluar/predecir. |
| Pesos/adapters ya entrenados | No directamente | El checkpoint no queda corrupto por este bug. Lo que cambia es como se mide. |
| KMP core | No | El kernel no inferia ni validaba estas acciones del Operator. |
| MCP/gRPC real | No como contrato | El contrato MCP/gRPC sigue siendo la frontera. El problema era que el pipeline de modelo no lo estaba replicando con suficiente rigor. |
| Persistencia/grafo | No | No se escribieron nodos o relaciones mal por este fallo. |

## Componentes No Afectados

Este problema no cambia ni invalida:

- `kernel_ingest`;
- `kernel_ask`;
- `kernel_near`;
- `kernel_trace`;
- `kernel_inspect`;
- paginacion temporal;
- multidimensionalidad;
- scopes de `about`;
- persistencia key-value;
- persistencia de grafo;
- proyeccion de eventos;
- TLS/mTLS;
- MCP como adaptador sobre KMP;
- gRPC como API principal;
- datos ya guardados en el kernel.

La frontera afectada es el pipeline externo del Operator:

```text
modelo -> predictor -> predictions.jsonl -> policy evaluator -> metricas/replay
```

## Impacto En Metricas

El impacto principal es sobre metricas que dependian de considerar una accion
como valida.

| Metrica | Impacto |
| --- | --- |
| `valid_predictions` | Puede estar inflada si una accion fuera de schema se conto como valida. |
| `invalid_predictions` | Puede estar subestimada. |
| `tool_accuracy` | Puede estar inflada si la herramienta era correcta pero la accion era invalida. |
| `primary_ref_accuracy` | Puede estar inflada si el ref principal era correcto pero la accion era invalida. |
| `scope_accuracy` | Puede estar inflada si el `about` era correcto pero la accion era invalida. |
| `exact_action_accuracy` | Menos afectada, porque compara el JSON completo. En el caso v10 no cambio. |
| `unbounded_tool_call_rate` | Puede ocultar acciones invalidas si primero se aceptan shapes parciales. Debe calcularse despues de shape validation estricta. |
| `MCP replay` | Debe reejecutarse para claims si la entrada venia de un predictor pre-strict. |

La metrica mas importante para release no es solo exactitud. Para un operador
de herramientas, la primera puerta es:

```text
La accion emitida cumple exactamente el contrato y es ejecutable.
```

Si esa puerta falla, no importa que la intencion parezca correcta.

## Correccion Concreta En V10

Artefactos evaluados:

- dataset: `/tmp/kernel-operator-sft-longmemeval-legacy-v10-operator-state-20260513`;
- adapter: `/tmp/kernel-operator-qwen05-lora-lme-v10-operator-state-4gpu-20260513`;
- predictions: `/tmp/kernel-operator-qwen05-predictions-lme-v10-operator-state-20260513`;
- strict eval: `/tmp/kernel-operator-qwen05-predictions-lme-v10-operator-state-20260513-policy-eval-strict.json`.

Resultado antes de endurecer el contrato:

| Metrica | Valor anterior |
| --- | ---: |
| Eval decisions | 60 |
| Valid predictions | 60 |
| Invalid predictions | 0 |
| Unbounded tool calls | 0 |
| Tool accuracy | 1.0000 |
| Primary ref accuracy | 1.0000 |
| Scope accuracy | 1.0000 |
| Stop accuracy | 1.0000 |
| Exact action accuracy | 0.9833 |

Resultado con contrato estricto:

| Metrica | Valor estricto |
| --- | ---: |
| Eval decisions | 60 |
| Valid predictions | 59 |
| Invalid predictions | 1 |
| Unbounded tool calls | 0 |
| Tool accuracy | 0.9773 |
| Primary ref accuracy | 0.9773 |
| Scope accuracy | 0.9773 |
| Stop accuracy | 1.0000 |
| Exact action accuracy | 0.9833 |

La unica fila invalida:

```text
longmemeval:run:lme-100-v6-20260505-a:temporal-reasoning:gpt4_2d58bcd6:read:0
```

Motivo:

```text
action.arguments_unexpected:final_refs
```

Interpretacion:

- el modelo eligio una accion semanticamente cercana;
- la herramienta era `kernel_ask`;
- el scope/ref principal eran correctos;
- pero la accion no era contractual porque llevaba `final_refs` dentro de
  `arguments`;
- por tanto debe contarse como invalida.

Esta correccion no cambia la lectura principal del v10: Qwen 0.5B con
`operator_state` aprendio casi toda la politica, incluido el uso de
`kernel_inspect`. Pero la cifra correcta para claims no es 60/60 validas, sino
59/60 validas con una invalidez de contrato.

## Resultado Del Rerun P0

El predictor v10 se reejecuto con el predictor estricto el 2026-05-14.

Artefactos nuevos:

- predictions dir: `/tmp/kernel-operator-qwen05-predictions-lme-v10-operator-state-strict-20260514`;
- policy eval: `/tmp/kernel-operator-qwen05-predictions-lme-v10-operator-state-strict-20260514-policy-eval.json`;
- Kubernetes job: `kop-qwen05-predict-lme-v10-state-strict`;
- manifest: `archive/k8s/kernel-operator-qwen05-predict-longmemeval-v10-operator-state-strict-20260514-job.yaml`.

Predictor summary:

| Metrica | Valor |
| --- | ---: |
| Selected rows | 60 |
| Predictions written | 59 |
| Failures written | 1 |
| Temperature | 0.0 |
| Stop after JSON | true |

Failure registrada:

| Campo | Valor |
| --- | --- |
| `step_id` | `longmemeval:run:lme-100-v6-20260505-a:temporal-reasoning:gpt4_2d58bcd6:read:0` |
| `reason` | `action.arguments_unexpected:final_refs` |
| `tool` | `kernel_ask` |
| causa | El modelo mezclo `stop.final_refs` dentro de `kernel_ask.arguments`. |

Policy eval sobre las predicciones estrictas:

| Metrica | Valor |
| --- | ---: |
| Eval decisions | 60 |
| Predictions present | 59 |
| Missing predictions | 1 |
| Invalid predictions | 0 |
| Unbounded tool calls | 0 |
| Tool accuracy | 0.9773 |
| Primary ref accuracy | 0.9773 |
| Scope accuracy | 0.9773 |
| Stop accuracy | 1.0000 |
| Exact action accuracy | 0.9833 |

Esta es la forma correcta del artefacto: la accion fuera de contrato no aparece
en `predictions.jsonl`; aparece en `failures.jsonl` con una razon explicita.
Por tanto, el evaluador ya no cuenta una accion invalida como prediccion valida.

## Revalidacion De Runs Publicables

El 2026-05-14 se revalidaron los runs candidatos o historicamente citables con
`kernel-operator-action-contract-v1`.

| Run | Trajectories | Predictions | Missing | Invalid | Unbounded | Exact | Decision |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| MemoryArena V6 holdout20, anonymized | 1,124 | 1,124 | 0 | 0 | 0 | 1.0000 | Release-grade offline eval. |
| MemoryArena V6 holdout20, de-anonymized | 1,124 | 1,124 | 0 | 0 | 0 | 1.0000 | Release-grade offline eval; replay-gated on 2026-05-14. |
| LongMemEval v8 clean | 60 | 56 | 4 | 2 | 0 | 0.7500 | Internal experiment only. |

MemoryArena V6 strict outputs:

- strict predictions: `/tmp/kernel-operator-qwen05-predictions-v6-holdout20-strict-20260514`;
- strict raw predictions: `/tmp/kernel-operator-qwen05-predictions-v6-holdout20-strict-20260514-raw`;
- anonymized eval: `/tmp/kernel-operator-qwen05-predictions-v6-holdout20-strict-20260514-policy-eval.json`;
- de-anonymized eval: `/tmp/kernel-operator-qwen05-predictions-v6-holdout20-strict-20260514-raw-policy-eval.json`;
- live MCP replay: `/tmp/kernel-operator-qwen05-predictions-v6-holdout20-strict-20260514-mcp-replay-full`;
- Kubernetes job: `kernel-operator-qwen05-predict-v6-holdout20-strict`;
- manifest: `archive/k8s/kernel-operator-qwen05-predict-v6-holdout20-strict-20260514-job.yaml`.

Both MemoryArena outputs report:

```json
{
  "action_validator": "kernel-operator-action-contract-v1",
  "schema_mode": "strict-no-additional-properties",
  "invalid_prediction_reasons": {},
  "counts": {
    "missing_predictions": 0,
    "invalid_predictions": 0,
    "unbounded_tool_calls": 0,
    "exact_action_correct": 1124
  }
}
```

MemoryArena V6 predictor summary:

| Metric | Value |
| --- | ---: |
| Selected rows | 1,124 |
| Predictions written | 1,124 |
| Failures written | 0 |
| Failure reasons | 0 |
| Validator | `kernel-operator-action-contract-v1` |
| Schema mode | `strict-no-additional-properties` |

MemoryArena V6 live replay against the public TLS endpoint:

| Metric | Value |
| --- | ---: |
| Selected trajectory steps | 1,124 |
| Executed MCP tool calls | 976 |
| Stop actions | 148 |
| Successful tool calls | 976 |
| Failed tool calls | 0 |
| Missing predictions | 0 |
| Invalid predictions | 0 |
| Unbounded tool calls | 0 |
| Missing expected ref rows | 0 |
| Missing expected refs | 0 |
| Extra observed ref rows | 848 |
| Extra observed refs | 7,216 |
| Partial result rows | 424 |
| Elapsed | 10m15.9s |

Action latency in the same replay:

| Action | Count | Avg ms | Max ms |
| --- | ---: | ---: | ---: |
| `kernel_near` | 424 | 1,302.7 | 2,517 |
| `kernel_inspect` | 424 | 110.0 | 198 |
| `kernel_trace` | 128 | 127.1 | 181 |
| `stop` | 148 | 0.0 | 0 |

The 424 partial result rows all came from `kernel_near`. That is expected for
the current page-aware traversal contract: the result is bounded, the page state
is explicit, and the replay records the partial result instead of silently
materializing unbounded history.

LongMemEval v8 strict output:

- eval: `/tmp/kernel-operator-qwen05-predictions-lme-v8-clean-20260513-policy-eval-strict-20260514.json`.

Invalid reasons:

```json
{
  "action.arguments has unexpected field `remaining_context_chars`": 1,
  "action.arguments missing required field `around`": 1
}
```

The four missing predictions came from strict predictor failures in the
pre-existing v8 artifact:

| Reason | Count |
| --- | ---: |
| `incomplete_json` | 1 |
| `invalid_json` | 1 |
| `missing_action_type` | 1 |
| `missing_tool` | 1 |

Interpretation:

- MemoryArena V6 survives the stricter contract without losing any claim.
- LongMemEval v8 clean should not be used as a public model-quality claim.
- LongMemEval v10 operator-state remains the stronger LongMemEval stress result
  because it exposes the final schema-mixing failure cleanly as one strict
  predictor failure.

## Por Que Importa Para Producto

Operator no es un generador de texto. Operator es una pieza que decide:

```text
Que herramienta KMP/MCP debo llamar ahora?
Con que argumentos acotados?
Debo seguir navegando, inspeccionar, trazar, preguntar o parar?
```

Si el output admite campos extra, aparecen varios riesgos:

- el modelo aprende que puede mezclar semanticas de acciones distintas;
- el evaluador premia acciones que no se podrian ejecutar de forma portable;
- el replay puede depender de tolerancia accidental del adaptador;
- las metricas dejan de reflejar calidad operacional real;
- una publicacion del modelo podria afirmar "valid tool use" sin que sea cierto;
- se rompe la paridad esperada entre MCP, gRPC y herramientas externas.

El producto necesita fail-fast:

```text
Una accion es valida solo si cumple el contrato exacto.
Si no, falla.
```

## Por Que Esto No Invalida KMP

KMP sigue teniendo la misma responsabilidad:

- memoria;
- recorrido temporal;
- dimensiones;
- prueba;
- inspeccion;
- auditabilidad;
- API tipada;
- paridad MCP/gRPC.

Operator esta por encima:

```text
estado visible de memoria -> decision de herramienta -> accion KMP/MCP
```

El bug estaba en la capa que mide si esa decision de herramienta cumple el
contrato. El kernel no se convierte en menos correcto porque un predictor
externo aceptara una accion fuera de schema.

## Politica Para Resultados Historicos

Todas las ejecuciones del Operator anteriores a esta auditoria deben
clasificarse asi:

| Tipo de resultado | Como tratarlo |
| --- | --- |
| Entrenamientos ya realizados | Mantener como baselines. |
| Loss/token accuracy | Mantener como senal de entrenamiento, no como proof de tool-use. |
| Policy eval pre-strict | Marcar como `pre-strict`; no usar como claim publicable sin revalidar. |
| Exact action accuracy pre-strict | Puede seguir siendo informativa, pero debe acompanarse del validador usado. |
| Zero invalid predictions pre-strict | No publicable hasta revalidar con contrato estricto. |
| Live replay pre-strict | Repetir si se quiere usar como claim externo. |

Regla:

```text
Si el artefacto no indica validador estricto, no es release-grade.
```

## Cambios Implementados

Se implemento una validacion estricta compartida en el testkit:

- `kernel_operator_action_shape_error`;
- `kernel_operator_is_valid_action_shape`;
- `kernel_operator_action_contract_error`.

La validacion comprueba:

- top-level keys exactas;
- herramienta soportada;
- campos requeridos;
- campos opcionales permitidos;
- ausencia de campos extra;
- tipos JSON correctos;
- `answer_policy` soportado;
- `dimensions.mode`;
- `dimensions.scope`;
- cursor requerido por herramienta;
- `include.raw=false` en inspect;
- limites positivos;
- presupuestos acotados;
- `window` temporal;
- `page` acotada en trace;
- `stop.final_refs` como array de strings.

El contrato se aplica ahora en:

| Pieza | Estado |
| --- | --- |
| `crates/kmp-testkit/src/kernel_operator.rs` | Fuente compartida del contrato estricto en Rust. |
| `crates/kmp-testkit/src/bin/kernel_operator_policy_eval.rs` | Usa el contrato estricto para contar invalid predictions. |
| `crates/kmp-testkit/src/bin/kernel_operator_llm_baseline.rs` | Usa el contrato estricto antes de aceptar acciones LLM. |
| `scripts/operator/predict_operator_sft.py` | Replica el contrato para rechazar outputs antes de escribir `predictions.jsonl`. |
| `docs/product/kernel-tool-operator-model-plan.md` | Documenta el resultado v10 corregido. |

Tests ejecutados:

```bash
python -m py_compile \
  scripts/operator/predict_operator_sft.py \
  scripts/operator/prepare_operator_sft_dataset.py

cargo test -p kmp-testkit kernel_operator -- --nocapture

cargo test -p kmp-testkit \
  --bin kernel_operator_policy_eval \
  -- --nocapture
```

Reevaluacion ejecutada:

```bash
cargo run -p kmp-testkit --bin kernel_operator_policy_eval -- \
  --trajectories /tmp/kernel-operator-sft-longmemeval-legacy-v10-operator-state-20260513/eval_model_trajectories.jsonl \
  --predictions /tmp/kernel-operator-qwen05-predictions-lme-v10-operator-state-20260513/predictions.jsonl \
  --output /tmp/kernel-operator-qwen05-predictions-lme-v10-operator-state-20260513-policy-eval-strict.json
```

## Riesgo Pendiente

Todavia hay duplicacion de contrato entre Rust y Python:

```text
Rust testkit validator <-> Python predictor validator <-> MCP tool schemas
```

Esto es aceptable para el corte actual porque el bug esta cerrado, pero no es
la forma ideal a largo plazo.

Riesgo:

- el schema MCP cambia y Python no se actualiza;
- el testkit valida una cosa y el predictor otra;
- el modelo se evalua contra una variante no identica al runtime real.

Direccion correcta:

```text
Una unica fuente de verdad de schema/contrato para MCP, gRPC, testkit y predictor.
```

Hasta que exista esa fuente unica, cada cambio de schema debe incluir:

- actualizacion en MCP schema;
- actualizacion en Rust validator;
- actualizacion en Python predictor;
- fixture de accion valida;
- fixture de accion invalida;
- policy eval de regresion.

## Que Debemos Hacer Ahora

### P0 - Antes De Cualquier Claim Publicable

1. Reejecutar el predictor v10 con el predictor estricto.

   Objetivo:

   ```text
   predictions.jsonl debe contener solo acciones contractuales.
   failures.jsonl debe contener la fila con final_refs invalido.
   ```

2. Recalcular policy eval v10 desde esos nuevos artefactos.

   Objetivo:

   ```text
   El summary oficial debe decir 59 predictions, 1 failure, 0 unbounded.
   ```

3. Revalidar los artefactos Operator que queramos mencionar publicamente.

   Minimo:

   - MemoryArena P1.11 V6 holdout20;
   - LongMemEval v8 clean;
   - LongMemEval v10 operator-state;
   - cualquier run que entre en model card, dataset card o articulo.

4. Marcar explicitamente los resultados anteriores como `pre-strict`.

   Donde:

   - `docs/product/kernel-tool-operator-model-plan.md`;
   - `scripts/operator/README.md`;
   - templates de Hugging Face si citan metricas;
   - cualquier tabla que diga "zero invalid predictions".

5. Bloquear publicacion de Operator si:

   - `invalid_predictions > 0`;
   - `unbounded_tool_calls > 0`;
   - el predictor acepta campos extra;
   - el replay usa outputs no validados;
   - el dataset no tiene split agrupado;
   - hay refs no visibles;
   - hay leakage de target/gold.

6. Asegurar que el replay MCP/gRPC solo consume predicciones estrictas.

   Si el artefacto de entrada contiene acciones invalidas, el replay no debe
   intentar ejecutarlas ni saltarlas silenciosamente.

Estado al cierre del P0 inmediato:

- v10 se reejecuto con predictor estricto;
- MemoryArena V6 se reejecuto con predictor estricto;
- MemoryArena V6 se de-anonimizo a refs reales;
- MemoryArena V6 paso policy eval anonimizada y raw con 1,124/1,124 exactas;
- MemoryArena V6 paso replay MCP real con 976/976 tool calls correctas;
- LongMemEval v8 queda marcado como experimento interno;
- los resultados pre-strict quedan fuera de claims publicables salvo
  revalidacion explicita.

### P1 - Mejorar La Robustez Del Pipeline

1. Anadir versionado explicito del validador al summary.

   Ejemplo:

   ```json
   {
     "action_validator": "kernel-operator-action-contract-v1",
     "schema_mode": "strict-no-additional-properties"
   }
   ```

   Estado: implementado en `kernel_operator_policy_eval` y en
   `predict_operator_sft.py`. El policy eval estricto del v10 se regenero con
   esta metadata.

2. Guardar razones de invalidez en el policy eval.

   Ahora sabemos contar invalidas, pero para debugging y articulo necesitamos
   agregados por motivo:

   ```text
   action.arguments_unexpected:final_refs -> 1
   ```

   Estado: implementado como `invalid_prediction_reasons`. Al reevaluar el
   artefacto pre-strict de v10 aparece:

   ```json
   {
     "action.arguments has unexpected field `final_refs`": 1
   }
   ```

3. Generar un report de contrato por run.

   Debe incluir:

   - total;
   - valid;
   - invalid;
   - missing;
   - unbounded;
   - invalid reasons;
   - action distribution;
   - tool/ref/scope/exact;
   - path a predictions;
   - path a failures;
   - validator version.

4. Reducir duplicacion de schema.

   Opciones:

   - generar validador Python desde JSON Schema MCP;
   - generar tests desde fixtures MCP;
   - serializar el contrato KMP/MCP a un artefacto versionado;
   - usar una libreria JSON Schema en el predictor.

5. Preparar constrained emission.

   El mejor siguiente paso para eliminar el ultimo fallo no es meter mas prompt.
   Es restringir la salida:

   - una unica accion;
   - keys cerradas;
   - herramienta elegida desde enum;
   - argumentos por herramienta;
   - stop separado de tool_call;
   - no campos extra.

6. Entrenar con ejemplos negativos de contrato.

   No para que el modelo "arregle" outputs invalidos, sino para que aprenda que
   mezclar campos de `stop` dentro de `tool_call` es una accion distinta y
   erronea.

### P2 - Productizar La Garantia

1. Publicar solo modelos con evaluator estricto reproducible.
2. Incluir contrato de salida completo en model card.
3. Incluir `known failure modes` con ejemplos reales.
4. Incluir replay MCP/gRPC como parte de la evaluacion.
5. Incluir dataset card con auditoria de leakage y refs visibles.
6. Incorporar el contrato a CI para que no vuelva a relajarse.

## Actualizacion Del Release Gate

La release publica del Operator requiere ahora:

| Gate | Requisito |
| --- | --- |
| Parser | Un unico JSON object con campo `action`. |
| Shape | Top-level keys exactas. |
| Tool schema | Campos exactos por herramienta, sin additional properties. |
| Bounds | Llamadas con limite/presupuesto acotado. |
| Refs | Solo refs visibles. |
| Invalid | `0` invalid predictions. |
| Unbounded | `0` unbounded tool calls. |
| Replay | MCP/gRPC real sin failures. |
| Metadata | Validator version registrada. |

## Runbook Practico

Revalidar predicciones existentes:

```bash
cargo run -p kmp-testkit --bin kernel_operator_policy_eval -- \
  --trajectories <operator-sft-dir>/eval_model_trajectories.jsonl \
  --predictions <operator-predictions-dir>/predictions.jsonl \
  --output <operator-policy-eval-strict>.json
```

Regenerar predicciones con predictor estricto:

```bash
python scripts/operator/predict_operator_sft.py \
  --dataset-jsonl <operator-sft-dir>/eval.jsonl \
  --model-id <base-model-id> \
  --adapter <operator-lora-dir> \
  --output <operator-predictions-strict-dir> \
  --batch-size 8 \
  --stop-after-json \
  --force
```

Auditar failures:

```bash
jq -r '.reason' <operator-predictions-strict-dir>/failures.jsonl | \
  sort | uniq -c | sort -nr
```

Aceptar un run para claim solo si:

```text
summary.failures == 0
policy_eval.counts.invalid_predictions == 0
policy_eval.counts.unbounded_tool_calls == 0
live_replay.mcp_failures == 0
live_replay.missing_expected_refs == 0
```

Para runs de investigacion se puede aceptar `failures > 0`, pero debe quedar
marcado como experimento interno y no como candidato de release.

## Criterios De Aceptacion Del Fix

Este problema se considera cerrado cuando:

- el predictor rechaza el ejemplo `kernel_ask.arguments.final_refs`;
- el policy evaluator cuenta esa prediccion como invalida;
- los tests de contrato cubren:
  - accion valida `kernel_near`;
  - accion valida `kernel_inspect`;
  - accion valida `kernel_ask`;
  - accion valida `stop`;
  - campo extra top-level;
  - campo extra en arguments;
  - llamada no acotada;
- los docs dicen que v10 corregido es 59/60 valido, no 60/60;
- la publication gate exige validador estricto;
- ninguna tabla publicable usa resultados `pre-strict` sin etiqueta.

## Estado Actual

Estado al 2026-05-14:

- contrato estricto implementado en Rust testkit;
- predictor Python endurecido;
- LLM baseline endurecida;
- v10 reevaluado;
- resultado corregido documentado;
- predictor v10 reejecutado con contrato estricto;
- `failures.jsonl` oficial producido con la fila `final_refs` fuera de schema;
- policy eval nuevo producido contra las predicciones estrictas;
- policy eval versionado con `action_validator` y `schema_mode`;
- razones de invalidez agregadas en `invalid_prediction_reasons`;
- predictor preparado para escribir `action_validator`, `schema_mode` y
  `failure_reasons` en nuevos summaries;
- MemoryArena V6 revalidado offline y con replay MCP real;
- LongMemEval v8 revalidado como no publicable;
- LongMemEval v10 revalidado como stress interno con 1 failure contractual
  correctamente aislado;
- falta un run mayor y fresco para publicacion/Hugging Face;
- falta reducir duplicacion de schema entre MCP, Rust y Python.

## Lectura De Producto

Este hallazgo mejora el producto.

No demuestra que el Operator no funcione. Demuestra que la barra correcta para
publicar un Operator debe ser mas alta que "parece que llamo la herramienta".

La tesis queda mas limpia:

```text
Underpass KMP ofrece una memoria navegable y auditable.
Operator aprende a operar esa memoria.
Pero Operator solo es fiable si sus acciones cumplen exactamente el contrato.
```

La direccion sigue siendo valida:

- KMP como sustrato de memoria;
- MCP/gRPC como entradas equivalentes;
- Operator como modelo pequeno especializado;
- validacion estricta como frontera de seguridad;
- replay real como prueba de que no estamos midiendo humo.
