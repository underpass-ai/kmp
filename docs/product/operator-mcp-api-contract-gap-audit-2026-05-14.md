# Operator MCP/API Contract Gap Audit - 2026-05-14

Esta auditoria documenta todos los gaps detectados entre:

- la API canonica KMP (`KernelMemoryService` gRPC);
- la entrada agentica MCP que usa un LLM;
- el contrato estricto que hoy usa el Operator;
- la cobertura real de las trayectorias MemoryArena V6 holdout20.

Objetivo de producto:

```text
El Operator debe aprender a usar KMP al maximo.
No debe aprender solo un subconjunto que pasa un benchmark.
```

Para el Operator, MCP es la forma agentica de consumir KMP. Por tanto, cuando se
diga "contrato MCP/API" en este documento, significa:

```text
MCP tools schema
  + runtime mapping MCP -> KernelMemoryService gRPC
  + semantica real del kernel desplegado
```

## Fuentes De Verdad

| Capa | Archivo |
| --- | --- |
| API gRPC canonica | `api/proto/underpass/rehydration/kernel/v1beta1/memory.proto` |
| MCP tools schema | `crates/kmp-mcp/src/protocol.rs` |
| MCP -> gRPC request mapping | `crates/kmp-mcp/src/grpc/requests/*.rs` |
| MCP structured output mapping | `crates/kmp-mcp/src/kmp.rs` |
| Operator Rust contract validator | `crates/kmp-testkit/src/kernel_operator.rs` |
| Operator Python predictor validator | `scripts/operator/predict_operator_sft.py` |
| Operator trajectories exporter | `crates/kmp-testkit/src/bin/kernel_operator_trajectory_export.rs` |
| Operator policy evaluator | `crates/kmp-testkit/src/bin/kernel_operator_policy_eval.rs` |
| Operator live replay | `crates/kmp-testkit/src/bin/kernel_operator_mcp_replay.rs` |

## Situacion Actual

El resultado MemoryArena V6 holdout20 es fuerte, pero estrecho.

Offline strict policy eval:

| Metric | Value |
| --- | ---: |
| Held-out decisions | 1,124 |
| Exact action accuracy | 1.0000 |
| Tool accuracy | 1.0000 |
| Primary ref accuracy | 1.0000 |
| Scope accuracy | 1.0000 |
| Stop accuracy | 1.0000 |
| Missing predictions | 0 |
| Invalid predictions | 0 |
| Unbounded tool calls | 0 |

Live MCP replay:

| Metric | Value |
| --- | ---: |
| Executed MCP tool calls | 976 |
| Stop actions | 148 |
| Successful tool calls | 976 |
| Failed tool calls | 0 |
| Missing expected refs | 0 |
| Partial result rows | 424 |

Esto demuestra que el Operator opera correctamente el subconjunto cubierto por
el holdout.

No demuestra todavia que opere KMP completo.

## P0: Cobertura 100% Del Contrato MCP/API

Este punto pasa a ser P0.

Si el Operator se coloca delante de KMP/MCP, no puede estrechar la API real sin
que eso sea una decision explicita de perfil. Si el modelo solo sabe usar una
parte de MCP, entonces el modelo no esta acelerando KMP completo: esta creando
una puerta mas pequena que puede mutilar capacidades reales del sistema.

Por tanto necesitamos una metrica de cobertura del contrato:

```text
operator_contract_coverage = capacidades MCP/API soportadas por el Operator
                             / capacidades MCP/API requeridas por el perfil
```

El objetivo para cualquier perfil publicable es:

```text
100% contract coverage
100% dataset target coverage
100% live replay coverage
```

Esto no significa que un unico modelo tenga que escribir, leer y auditar desde
el primer dia. Significa que cada perfil declarado debe estar completo.

Perfiles razonables:

| Perfil | Alcance |
| --- | --- |
| `operator-read` | `wake`, `ask`, `near`, `goto`, `rewind`, `forward`, `trace`, `inspect`, `stop` |
| `operator-audit` | `operator-read` + raw audit refs/raw inspect bajo politica explicita |
| `operator-write` | `write_memory`/`ingest` + lectura previa para prueba de relacion |
| `operator-full` | Todo el surface MCP/KMP expuesto al LLM |

La cobertura debe ser visible en CI/local checks. Si una herramienta nueva entra
en MCP, o si cambia la semantica de dimensiones, cursores, paginacion o raw
access, la cobertura debe bajar hasta que exista soporte del Operator y dataset
que lo ejercite.

Medicion actual del reporter `kernel_operator_contract_coverage`:

| Scope | Coverage | Lectura |
| --- | ---: | --- |
| MCP global tools desde `operator-read` | 80.00% | `kernel_ingest` y `kernel_write_memory` no pertenecen al perfil lector actual. |
| MCP global tools desde `operator-full` | 100.00% | El contrato full ya cubre todas las tools MCP publicadas. |
| `operator-read` contract | 100.00% | El contrato ya puede expresar el perfil lector completo. |
| `operator-full` contract | 100.00% | El contrato ya expresa `kernel_ingest`, `kernel_write_memory`, relation quality y read_context proof. |
| MemoryArena V6 target capabilities | 41.67% | El dataset actual no cubre todo el perfil lector. |
| MemoryArena V6 target capabilities contra full | 35.71% | El dataset actual no contiene acciones de escritura. |
| KMP conformance full target capabilities | 100.00% | Suite sintetica v7 de 61 trajectories para cubrir todo el contrato full. |
| P1.11 + conformance v7 read train target capabilities | 100.00% | Split capability-aware; 24/24 capacidades read en train. |
| P1.11 + conformance v7 read eval target capabilities | 100.00% | Split capability-aware; 24/24 capacidades read en eval. |

El dato importante ya no es solo el contrato. El contrato `operator-full` puede
expresar todo KMP/MCP, incluida escritura. El gap principal pasa a ser de datos:
el modelo no ha sido entrenado/evaluado todavia con suficientes casos para decir
que domina el perfil full. Para `operator-read`, el primer dataset mixto con
cobertura 100% en train y eval ya existe; todavia falta entrenar, evaluar y
replayar contra MCP real antes de convertirlo en claim de modelo.

## P0: Datasets Por Caso De Uso MCP/API

El gap principal ahora no es solo de modelo. Es de datos.

Necesitamos sets de entrenamiento/evaluacion por caso de uso de la API/MCP para
cubrir el 100% del contrato declarado. MemoryArena V6 valida un subconjunto muy
util, pero no contiene todos los casos que KMP expone.

Suites necesarias:

| Suite | Objetivo |
| --- | --- |
| `kmp-read-wake` | Aprender cuando usar `kernel_wake` para continuar trabajo desde memoria. |
| `kmp-read-ask` | Aprender `kernel_ask` con `evidence_or_unknown`, `show_conflicts` y `best_effort`. |
| `kmp-temporal-ref` | `near/goto/rewind/forward` con cursor por `ref`. |
| `kmp-temporal-time` | `near/goto/rewind/forward` con cursor por `time`. |
| `kmp-temporal-sequence` | `near/goto/rewind/forward` con cursor por `sequence`. |
| `kmp-dimensions-mode` | `all`, `only`, `except`, incluyendo casos invalidos fail-fast. |
| `kmp-dimensions-scope` | `current_about`, `abouts`, `all_abouts`, incluyendo `ABOUTS` sin lista como error. |
| `kmp-trace-pagination` | Primera pagina y continuacion con `page.cursor`. |
| `kmp-window-policy` | Expandir ventana, reducir ventana, cambiar de herramienta y parar. |
| `kmp-inspect-policy` | Inspect ligero, inspect tipado completo, raw=false por defecto. |
| `kmp-audit-raw` | Raw refs/raw inspect solo si se declara perfil auditor. |
| `kmp-write-memory` | Escribir texto + relacion + why + evidencia + read_context proof. |
| `kmp-ingest-canonical` | Ingest canonico, idempotency, dimensiones, entradas, relaciones y evidencia. |

Cada suite debe producir:

- trajectories model-facing sin gold leakage;
- targets estrictos;
- policy eval offline;
- replay real MCP/gRPC;
- cobertura por herramienta;
- cobertura por cursor;
- cobertura por modo/scope de dimensiones;
- cobertura de paginacion;
- cobertura de politica de ventana;
- errores esperados fail-fast cuando el caso sea invalido.

Hasta tener estas suites, no se debe afirmar que Operator sabe usar KMP al
maximo. La afirmacion correcta sigue siendo:

```text
Operator sabe operar el subconjunto KMP/MCP cubierto por el modelo evaluado.
```

Update 2026-05-14:

```text
El dataset `operator-read` ya cubre 100% del perfil read en train y eval.
La afirmacion de modelo aun espera entrenamiento, policy eval y live replay.
```

## Auditoria De Muestras: No Entrenar A Inventar

La ejecucion de conformance v4 encontro un gap importante del corpus.

No era correcto interpretar todos los fallos como incapacidad del modelo. Varias
muestras pedian al Operator emitir una accion con campos que no estaban
presentes en el `visible_state`.

Eso rompe una regla basica de KMP:

```text
El Operator puede seleccionar, copiar, acotar, transformar de forma controlada
o parar. No debe inventar memoria.
```

### Caso `kernel_ingest`

El target `kernel_ingest` contiene un payload completo:

- `memory.dimensions`;
- `memory.entries`;
- `memory.relations`;
- `memory.evidence`;
- `provenance`;
- `idempotency_key`;
- `dry_run`.

En conformance v4, el `visible_state` de algunos casos solo decia:

```json
{"canonical_payload_ready": true}
```

Eso no basta. Si el payload completo no esta visible, el modelo no puede
reconstruirlo de forma honesta. El resultado observado fue exactamente el que
debia esperarse:

- el modelo eligio bien `kernel_ingest` en 4/4;
- pero cambio entradas, relaciones, textos, ids e `idempotency_key`;
- la exactitud fue 0/4.

Lectura correcta:

```text
El modelo aprendio que debe usar ingest, pero el dataset le obligaba a inventar
el contenido del nodo.
```

Correccion v5:

```json
{
  "canonical_payload_ready": true,
  "canonical_payload": { "...": "payload completo que debe pasar a kernel_ingest" }
}
```

Si `canonical_payload` no esta visible, el target no debe ser `kernel_ingest`.
Debe ser `stop` con una razon fail-fast, por ejemplo
`canonical_payload_not_visible`.

### Caso `kernel_write_memory`

`kernel_write_memory` no es solo "guardar texto".

El target incluye:

- `about`;
- `intent`;
- `actor`;
- `observed_at`;
- `scope`;
- `current.kind`;
- `current.summary`;
- `current.evidence`;
- `connect_to`;
- relacion (`rel`, `class`, `why`, `evidence`, `confidence`);
- `read_context`;
- `semantic_delta` cuando aplica;
- `idempotency_key`;
- `options`.

En v4, algunos casos complejos solo exponian algo como:

```json
{
  "draft_write": {
    "intent": "record_memory",
    "relation_requirement": "use only strict kernel_write_memory fields"
  }
}
```

Eso tampoco basta. El modelo no puede saber que `current`, `scope`,
`connect_to`, `semantic_delta` e `idempotency_key` exactos debe emitir si no
estan visibles.

Correccion v5:

```json
{
  "draft_write": {
    "intent": "record_memory",
    "prepared_arguments": { "...": "argumentos completos de kernel_write_memory" },
    "relation_requirement": "use only strict kernel_write_memory fields"
  }
}
```

Esto convierte la muestra en una tarea legitima de copia/ejecucion:

```text
hay un draft preparado y valido -> decide usar kernel_write_memory y emite ese
payload sin inventar campos.
```

Si el draft no esta preparado, la muestra debe ensenar otra politica:

```text
leer mas contexto, inspeccionar refs, trazar, o parar.
```

### Decision De Producto Sobre Escritura

Aunque v5 corrige el problema de "pedir inventar", no debe interpretarse como
el camino final para entrenar escritura.

La escritura real en KMP es mas compleja que copiar un payload preparado. El
flujo correcto es:

```text
1. El LLM/writer identifica el contexto de escritura.
2. Lee memoria cercana con KMP: near, inspect, trace, rewind/forward si hace falta.
3. Decide si el nuevo nodo debe conectarse con memoria previa.
4. Si hay una relacion rica justificada, escribe texto + relacion + why + evidencia.
5. Si no hay relacion rica justificable, cae a la relacion anemica determinista
   por defecto, normalmente follows.
6. El kernel valida alcance, prueba, contrato, idempotencia y auditabilidad.
```

La parte no determinista es importante:

```text
El porque de una relacion rica lo decide el writer/LLM que usa KMP.
El kernel no debe inferir ese significado.
```

El kernel puede validar que la relacion sea honesta:

- el target existe y esta dentro del alcance correcto;
- el writer ha leido/inspeccionado la evidencia necesaria;
- `why` y `evidence` estan presentes;
- la relacion cumple el contrato;
- no se han inventado campos;
- la decision queda auditable.

Pero el kernel no debe decidir por si mismo que una entrada `contradicts`,
`supersedes`, `chosen_because`, `updates_state` o `contributes_to` otra. Esa
decision pertenece al writer.

Por tanto, el plan queda dividido:

| Area | Estado |
| --- | --- |
| `operator-read` | P0 actual. Debe aprender a navegar KMP al maximo. |
| `operator-full` con write preparado | Util como test de contrato y anti-invencion, no como producto final de escritura. |
| `writer inteligente` | Diseño separado. Primero debe leer contexto y despues decidir relacion. La decision semantica rica no debe recaer inicialmente en un modelo 0.5B. |
| fallback anemico | Determinista y permitido cuando no hay relacion rica honesta. |

Regla para los datasets de escritura:

```text
No entrenar al modelo a inventar memoria.
No entrenar escritura como simple autocompletado de campos.
Entrenar primero la politica de lectura/contexto que permite escribir bien.
```

### Teacher De Escritura

Para writer semantico, el dataset no debe asumir que un modelo pequeno de
0.5B puede decidir relaciones ricas de forma fiable.

El reparto correcto es:

| Pieza | Responsabilidad |
| --- | --- |
| GPT-5.5 teacher | Generar decisiones de escritura semantica offline: relacion, `why`, evidencia y fallback cuando no hay prueba suficiente. |
| Operator 0.5B | Solo operar KMP: decidir que herramienta llamar, con que limites, cuando inspeccionar, cuando trazar, cuando parar y cuando escalar. |
| Kernel | Validar contrato, alcance, evidencia, idempotencia y auditabilidad. No inferir significado. |

El Operator 0.5B no debe ser tratado como autor de relaciones ricas en la
primera version. Puede aprender:

- que necesita leer antes de escribir;
- que refs son candidatas visibles;
- que una escritura preparada puede ejecutarse si ya contiene relacion y prueba;
- que puede ejecutar una relacion anemica cuando ya viene preparada o la
  politica explicita lo permite sin inventar significado;
- que debe escalar a un modelo teacher/razonador grande cuando falta decision
  semantica.

En otras palabras: el 0.5B no escribe significado. Aprende el protocolo de uso
del kernel.

Las muestras teacher de escritura deben conservar procedencia obligatoria:

- `label_source = gpt5_5_teacher`;
- `teacher_model` con el identificador exacto usado en la ejecucion;
- `teacher_prompt_version`;
- herramientas KMP/MCP permitidas durante la lectura;
- refs inspeccionadas;
- evidencia citada por cada relacion rica;
- razon explicita cuando la decision sea fallback anemico o escalado.

Si el teacher no esta disponible, no se genera ese dataset. No hay fallback
silencioso a otro modelo para muestras de escritura semantica.

Hasta cerrar ese diseno, no se debe lanzar un entrenamiento publicable de
`operator-full` con escritura. El siguiente entrenamiento recomendado vuelve a
ser `operator-read`.

### Caso `about` Vs `ref`

Otro problema detectado fue la anonimizacion.

En raw:

```text
about = incident:mobile-login
current_ref = incident:mobile-login:draft:...
```

En v4 model-facing, `about` podia convertirse en `ref_0001`, igual que las refs
de nodos. Eso mezcla conceptos:

- `about` es el ambito/caso sobre el que se trabaja;
- `ref` es una referencia a un nodo/entrada/evidencia.

Para KMP esa diferencia importa mucho. Si el dataset borra esa diferencia, el
modelo puede aprender que `about` es una ref mas y usar `current_ref` como
`arguments.about`.

Correccion v5:

```text
about -> about_0001
node refs -> ref_0001, ref_0002, ...
```

La regla queda:

```text
arguments.about debe copiar el top-level about, pero no debe confundirse con
current_ref ni con refs de nodos.
```

### Como Leer Los Resultados v4

Conformance v4 sigue siendo util, pero como auditoria del corpus.

Resultados relevantes:

| Area | Resultado v4 | Lectura |
| --- | ---: | --- |
| Contract coverage | 100% | El contrato full esta representado. |
| `goal` en prompt | si | El bug de prompt v3 esta corregido. |
| Exact action | 14/58 | No publicable. |
| Missing predictions | 10/58 | El modelo rompe formato en casos complejos. |
| `kernel_ingest` tool | 4/4 | Elige la herramienta correcta. |
| `kernel_ingest` exact | 0/4 | El payload no estaba visible; el modelo invento. |
| `kernel_write_memory` exact | 0/8 | Los drafts complejos no estaban suficientemente materializados. |
| Stop/fail-fast | fuerte | El modelo aprende bien a parar cuando el target es claro. |

Por tanto, v4 no debe usarse para afirmar:

```text
Qwen 0.5B no sabe escribir KMP.
```

La afirmacion correcta es:

```text
El corpus v4 todavia contenia muestras que pedian inventar memoria. v5 corrige
esa frontera haciendo visible el payload que debe emitirse o convirtiendo el
caso en fail-fast.
```

## Medicion De Aporte Por Muestra

Para no seguir anadiendo datos a ciegas, el policy evaluator emite un detalle
por muestra con:

- `step_id`;
- `target_capability_key`;
- target action;
- predicted action;
- estado de prediccion: `valid`, `missing`, `invalid`;
- aciertos por componente:
  - action type;
  - tool;
  - refs primarias;
  - scope;
  - cursor mode;
  - window;
  - limit;
  - page continuation;
  - stop;
  - exact action.

Esto permite comparar dos ejecuciones sobre el mismo probe set y clasificar el
efecto de un lote de muestras:

| Verdict | Significado |
| --- | --- |
| `improved` | El nuevo entrenamiento mejora esa muestra/probe. |
| `regressed` | El nuevo entrenamiento rompe algo que antes iba mejor. |
| `stable_correct` | La muestra ya estaba resuelta y sigue resuelta. |
| `stable_gap` | La muestra sigue fallando; faltan datos, prompt o capacidad. |

La regla para aceptar nuevas muestras pasa a ser:

```text
Un lote aporta si reduce stable_gap/improves en su capacidad objetivo sin crear
regresiones relevantes en capacidades ya verdes.
```

## Herramientas MCP/API

MCP expone actualmente:

| MCP tool | gRPC/API target | Clase |
| --- | --- | --- |
| `kernel_ingest` | `KernelMemoryService.Ingest` | write canonical |
| `kernel_write_memory` | compila a `kernel_ingest` | write helper |
| `kernel_wake` | `KernelMemoryService.Wake` | read/continuation |
| `kernel_ask` | `KernelMemoryService.Ask` | deterministic evidence QA |
| `kernel_goto` | `KernelMemoryService.Goto` | temporal movement |
| `kernel_near` | `KernelMemoryService.Near` | temporal neighborhood |
| `kernel_rewind` | `KernelMemoryService.Rewind` | temporal movement |
| `kernel_forward` | `KernelMemoryService.Forward` | temporal movement |
| `kernel_trace` | `KernelMemoryService.Trace` | proof/path traversal |
| `kernel_inspect` | `KernelMemoryService.Inspect` | node/detail inspection |

El contrato actual del Operator permite:

| Operator action | Estado |
| --- | --- |
| `kernel_ask` | Permitido por contrato, no cubierto en V6 holdout. |
| `kernel_goto` | Permitido por contrato, no cubierto en V6 holdout. |
| `kernel_near` | Cubierto y replay real limpio. |
| `kernel_rewind` | Permitido por contrato, no cubierto en V6 holdout. |
| `kernel_forward` | Permitido por contrato, no cubierto en V6 holdout. |
| `kernel_trace` | Cubierto, sin page continuation en targets. |
| `kernel_inspect` | Cubierto con `raw=false`. |
| `stop` | Cubierto. |
| `kernel_wake` | Permitido por contrato y cubierto por conformance v4; no cubierto en V6 holdout. |
| `kernel_ingest` | Permitido por contrato full y cubierto por conformance v4; no cubierto en V6 holdout. |
| `kernel_write_memory` | Permitido por contrato full y cubierto por conformance v4; no cubierto en V6 holdout. |

Decision pendiente:

```text
Si queremos un Operator que use KMP al maximo, el contrato ya permite un
Operator full-KMP. La decision de producto sigue siendo si publicamos un unico
modelo full o perfiles separados: read-operator, writer-operator,
audit-operator.
```

## Cobertura Real Del Holdout V6

Target actions en MemoryArena V6 holdout20:

| Target | Count |
| --- | ---: |
| `kernel_near` | 424 |
| `kernel_inspect` | 424 |
| `kernel_trace` | 128 |
| `stop` | 148 |

Por modo:

| Mode | Target | Count |
| --- | --- | ---: |
| `read` | `kernel_near` | 148 |
| `read` | `kernel_inspect` | 148 |
| `read` | `kernel_trace` | 128 |
| `read` | `stop` | 148 |
| `write_context_read` | `kernel_near` | 276 |
| `write_context_read` | `kernel_inspect` | 276 |

Allowed tools en cada trajectory incluian:

- `kernel_ask`;
- `kernel_forward`;
- `kernel_goto`;
- `kernel_inspect`;
- `kernel_near`;
- `kernel_rewind`;
- `kernel_trace`.

Pero el target no uso:

- `kernel_ask`;
- `kernel_goto`;
- `kernel_rewind`;
- `kernel_forward`.

Esto significa que el modelo puede tener esas tools en `allowed_tools`, pero el
score actual no prueba que sepa usarlas.

## Gaps De Tool Coverage

### `kernel_wake`

API/MCP:

- existe;
- requiere `about`;
- acepta `role`, `intent`, `dimensions`, `depth`, `budget`;
- devuelve un wake packet para continuar trabajo desde memoria.

Operator:

- permitido por contrato;
- cubierto por conformance v4;
- no aparece en MemoryArena V6 target;
- no hay replay.

Gap:

```text
MemoryArena V6 no demuestra cuando despertar contexto en vez de hacer ask/near.
Conformance v4 cubre la forma contractual, pero falta replay real y mas
variantes de politica.
```

P0/P1 decision:

- si `wake` es parte del operador lector, meterlo en el perfil read;
- si `wake` es un helper de cliente, dejarlo fuera pero documentarlo.

### `kernel_ask`

API/MCP:

- requiere `about`, `question`;
- `answer_policy` es opcional y default `evidence_or_unknown`;
- `dimensions` es opcional;
- `budget` es opcional;
- devuelve evidencia determinista, no respuesta generativa libre.

Operator:

- lo permite;
- exige `answer_policy`;
- exige `dimensions`;
- no aparece en MemoryArena V6 target;
- LongMemEval v10 lo ejercito y expuso el fallo `final_refs` dentro de
  `kernel_ask.arguments`, ya corregido por predictor estricto.

Gaps:

- falta cobertura estable en un holdout publicable;
- falta medir cuando conviene `ask` frente a `near + inspect`;
- falta policy para `show_conflicts` y `best_effort`;
- falta QA multi-session limpio sin depender del reader generativo.

### `kernel_near`

API/MCP:

- requiere `about`, `around`;
- `around` puede ser por `ref`, `time` o `sequence`;
- acepta `window`, `limit`, `dimensions`, `include`, `depth`, `budget`;
- devuelve `entries`, `proof`, `coverage`, `raw_refs`, `warnings`, `page`.

Operator:

- cubierto;
- acepta cursor `around.ref`, `around.time` y `around.sequence`;
- exige `window`, `limit`, `dimensions`, `include`, `budget`;
- aprende dos presets:
  - `read`: entries 12, tokens 2400, depth 3, before 6, after 0;
  - `write_context_read`: entries 8, tokens 1800, depth 2, before 3, after 0.

Gaps:

- MemoryArena V6 no usa cursor `time`;
- MemoryArena V6 no usa cursor `sequence`;
- MemoryArena V6 no aprende expansion/reduccion dinamica de ventana;
- MemoryArena V6 no aprende continuation policy sobre `page.has_more`;
- no aprende `after_entries > 0` en este holdout;
- MemoryArena V6 no aprende scopes `abouts` o `all_abouts`;
- conformance v4 cubre esos modos como contrato, pero todavia necesita mas
  variantes para politica estable.

### `kernel_goto`

API/MCP:

- salta al estado en un cursor temporal;
- cursor puede ser `ref`, `time`, `sequence`;
- acepta `window`, `limit`, `dimensions`, `include`, `budget`.

Operator:

- permitido por contrato;
- cubierto por conformance v4;
- no cubierto en V6 targets;
- validador acepta `at.ref`, `at.time` y `at.sequence`.

Gaps:

- no hay training/eval real en un benchmark no sintetico;
- no hay decision policy para "estado conocido en ese momento";
- no hay benchmark de known-at usando `goto`.

### `kernel_rewind`

API/MCP:

- mueve hacia atras desde un cursor;
- cursor puede ser `ref`, `time`, `sequence`;
- acepta `window`, `limit`, `dimensions`, `include`, `budget`.

Operator:

- permitido por contrato;
- cubierto por conformance v4;
- no cubierto en V6 targets;
- validador acepta `from.ref`, `from.time` y `from.sequence`.

Gaps:

- no hay training/eval real en un benchmark no sintetico;
- no hay policy para buscar decisiones previas;
- no hay policy de profundidad temporal.

### `kernel_forward`

API/MCP:

- mueve hacia adelante desde un cursor;
- cursor puede ser `ref`, `time`, `sequence`;
- acepta `window`, `limit`, `dimensions`, `include`, `budget`.

Operator:

- permitido por contrato;
- cubierto por conformance v4;
- no cubierto en V6 targets;
- validador acepta `from.ref`, `from.time` y `from.sequence`.

Gaps:

- no hay training/eval real en un benchmark no sintetico;
- no hay policy para buscar cambios posteriores;
- no hay policy para actualizar estado vigente.

### `kernel_trace`

API/MCP:

- requiere `from`, `to`;
- `goal`, `role`, `budget`, `page` son opcionales;
- devuelve `trace`, `warnings`, `page`;
- `page.has_more=true` implica continuar con `page.next_cursor`.

Operator:

- cubierto;
- exige `budget`;
- permite `page`;
- conformance v4 cubre primera pagina y continuacion con `page.cursor`;
- V6 target/predictions: 128 `kernel_trace`, todos sin `page`.

Gaps:

- MemoryArena V6 no aprende continuation de trace;
- MemoryArena V6 no aprende `page.cursor`;
- no aprende cuando aumentar `page.entries`;
- no aprende cuando parar con trace parcial;
- no consume warnings/page como senal de siguiente decision.

### `kernel_inspect`

API/MCP:

- requiere `ref`;
- `include` es opcional;
- `include.raw=true` esta soportado por MCP/API;
- devuelve object, links, evidence, warnings, raw.

Operator:

- cubierto;
- exige `include`;
- exige `raw=false`;
- target actual siempre:

```json
{"details": true, "incoming": true, "outgoing": true, "raw": false}
```

Gaps:

- no sabe pedir raw audit refs;
- no aprende variantes ligeras (`details` only, links only);
- no aprende tradeoff coste/detalle;
- no aprende inspect de raw cuando auditoria lo necesita.

Decision:

- `raw=false` es conservador y razonable para seguridad;
- si queremos Operator auditor, `raw=true` debe ser otro modo/perfil con tests.

### `kernel_ingest`

API/MCP:

- low-level canonical write;
- requiere `about`, `memory`, `idempotency_key`;
- escribe dimensiones, entries, relations, evidence, provenance.

Operator:

- permitido por contrato full;
- cubierto por conformance v4;
- no cubierto por MemoryArena V6.

Gap:

```text
El contrato ya sabe representar escritura canonical KMP y conformance v4 la
ejercita. Falta pasar de cobertura sintetica a corpus amplio y replay real de
writer.
```

Decision:

- probablemente no debe entrar en el primer read-operator;
- si se mete, debe ser un writer-operator separado con validacion de relaciones.

### `kernel_write_memory`

API/MCP:

- helper writer-friendly;
- valida intent, relation quality, read_context proof;
- compila a `kernel_ingest`;
- soporta `dry_run` y commit.

Operator:

- permitido por contrato full;
- cubierto por conformance v4;
- el smart writer actual lo usa como pipeline externo, no como decision del
  Operator pequeno.

Gap:

```text
El contrato ya sabe representar escritura inteligente por MCP y conformance v4
la ejercita. Falta mas corpus y replay real antes de poner un Operator writer
delante de KMP.
```

Decision:

- separar `Operator-read` de `Operator-write`;
- no mezclar hasta que `Operator-read` domine KMP temporal/paginado.

## Gaps De Campo Y Semantica

### Campos Opcionales En API, Requeridos Por Operator

| Tool | API/MCP | Operator |
| --- | --- | --- |
| `kernel_ask.answer_policy` | opcional, default `evidence_or_unknown` | requerido |
| `kernel_ask.dimensions` | opcional | requerido |
| temporal `dimensions` | opcional | requerido |
| temporal `include` | opcional | requerido |
| temporal `limit` | opcional | requerido |
| temporal `budget` | opcional/default | requerido |
| temporal `window` | opcional | requerido |
| `kernel_trace.budget` | opcional/default | requerido |
| `kernel_trace.page` | opcional | opcional |
| `kernel_inspect.include` | opcional | requerido |

Esto es un subconjunto estricto. Es aceptable para bounded tool-use, pero debe
quedar documentado como un perfil:

```text
Operator strict profile != todo el espacio valido de MCP/API.
```

### Cursor Temporal

| Cursor | API/MCP | Operator |
| --- | --- | --- |
| `ref` | soportado | soportado |
| `time` | soportado | soportado |
| `sequence` | soportado | soportado |

El gap ya no es de contrato, sino de corpus: MemoryArena V6 no ejercita
`time`/`sequence`; conformance v4 si los cubre.

### Dimension Selection

MCP/gRPC valida semantica:

| Regla | API/MCP runtime | Operator validator actual |
| --- | --- | --- |
| `mode=all` no lleva `include/exclude` | valida | valida |
| `mode=only` requiere `include` | valida | valida |
| `mode=except` requiere `exclude` | valida | valida |
| `scope=current_about` no lleva `abouts` | valida | valida |
| `scope=abouts` requiere `abouts` no vacio | valida | valida |
| `scope=all_abouts` no lleva `abouts` | valida | valida |

Este gap P0 de contrato queda cerrado en el validador Rust. La regla sigue
siendo importante:

```text
Una accion no debe pasar policy eval si MCP/gRPC la rechazaria de forma
determinista.
```

Acciones abiertas:

- mantener fixtures validos e invalidos por regla;
- mantener paridad con el predictor Python;
- contar `invalid_prediction_reasons` por regla cuando el modelo falle.

### Temporal Window

API/MCP:

- `window.before_entries` opcional;
- `window.after_entries` opcional;
- si faltan, default 0;
- `before_seconds/after_seconds` se rechazan en este corte.

Operator:

- exige `before_entries`;
- exige `after_entries`;
- no usa `after_entries > 0` en V6;
- no soporta seconds, igual que API runtime.

Gap:

- no domina windows simetricas;
- no domina forward-looking window;
- no aprende reducir/ampliar window.

### Limit

API/MCP:

- `limit.entries` opcional;
- `limit.tokens` opcional;
- si faltan, el kernel/mapping puede aplicar default.

Operator:

- para temporal exige ambos;
- valida boundedness maxima:
  - entries <= 64;
  - tokens <= 16,000.

Gap:

- no aprende tradeoff entries/tokens;
- no aprende token budget dinamico;
- no aprende reducir tokens por coste;
- no aprende aumentar entries si `has_more=true`.

### Budget

API/MCP:

- opcional;
- mapping aplica defaults:
  - ask: 2400 tokens, depth 2;
  - temporal: 2400 tokens, depth 3;
  - trace: 1600 tokens, depth 1.

Operator:

- exige budget en temporal y trace;
- acepta `tokens`, `depth`, `detail`;
- no entrena `detail`.

Gap:

- no aprende `budget.detail`;
- no aprende presupuesto segun complejidad;
- no aprende stop por presupuesto bajo.

### Include

API/MCP:

- temporal include opcional;
- inspect include opcional;
- defaults false/true segun mapping.

Operator:

- exige include explicito;
- temporal actual usa evidence=true, relations=true, raw_refs=false;
- inspect actual usa details=true, incoming=true, outgoing=true, raw=false.

Gaps:

- no aprende variants ligeras;
- no aprende `raw_refs=true`;
- no aprende `raw=true`;
- no aprende coste/beneficio de links/evidence.

## Gaps De Output MCP/API Frente A Visible State Del Operator

MCP/API devuelve mas informacion de la que el Operator ve hoy en V6.

### Temporal Output

API/MCP puede devolver:

- `summary`;
- `temporal.requested`;
- `temporal.resolved`;
- `coverage.requested`;
- `coverage.included`;
- `coverage.missing`;
- `entries`;
- `proof`;
- `warnings`;
- `raw_refs`;
- `page`.

Visible state V6 contiene principalmente:

- `candidate_refs`;
- `candidate_ref_details`;
- `current_ref`;
- `known_refs`;
- `last_observed_refs`;
- `last_tool`;
- `remaining_budget`;
- `writer` metadata.

Gaps:

- no incluye `coverage.missing`;
- no incluye warnings;
- no incluye page metadata en V6;
- no incluye token/page cost real;
- no incluye proof completeness;
- no incluye enough/sufficient evidence labels;
- no incluye razon explicita de por que target amplia/reduce/para.

### Trace Output

API/MCP devuelve:

- `trace`;
- `warnings`;
- `page`.

Gaps:

- Operator no aprende continuacion de trace;
- no aprende `page.next_cursor`;
- no aprende si un trace parcial ya basta;
- no aprende a pedir mas path cuando falta causalidad.

### Inspect Output

API/MCP devuelve:

- object;
- links incoming/outgoing;
- evidence;
- warnings;
- raw.

Gaps:

- visible state comprime esto a refs/candidates;
- no aprende variaciones de include;
- no aprende auditoria raw;
- no aprende usar warning como decision.

## Gaps De Evaluacion

El evaluator actual mide:

- action type;
- tool;
- primary refs;
- scope;
- stop;
- exact action;
- invalid predictions;
- unbounded tool calls.

No mide todavia:

- `dimension_semantic_validity`;
- `cursor_mode_accuracy`;
- `time_cursor_accuracy`;
- `sequence_cursor_accuracy`;
- `window_policy_accuracy`;
- `expand_window_accuracy`;
- `shrink_window_accuracy`;
- `continue_page_accuracy`;
- `stop_when_sufficient_accuracy`;
- `over_read_rate`;
- `under_read_rate`;
- `raw_access_policy_accuracy`;
- `wake_vs_ask_vs_near_accuracy`;
- `write_dry_run_vs_commit_accuracy`;
- `reader/plugin handoff correctness`.

P0 evaluator additions:

| Metric | Meaning |
| --- | --- |
| `api_contract_valid` | action passes full MCP/gRPC semantic rules |
| `operator_profile_valid` | action passes stricter operator profile |
| `cursor_mode_accuracy` | ref/time/sequence choice matches target |
| `window_shape_accuracy` | before/after entries match target class |
| `limit_policy_accuracy` | entries/tokens match target class |
| `continue_page_accuracy` | follows `page.next_cursor` when target says continue |
| `stop_when_sufficient_accuracy` | stops after enough evidence |
| `over_read_rate` | reads more context than target policy |
| `under_read_rate` | stops or narrows before evidence is sufficient |
| `raw_access_violation_rate` | asks raw when policy forbids it |

## Gaps De Dataset

Current V6 dataset:

- no target `kernel_ask`;
- no target `kernel_goto`;
- no target `kernel_rewind`;
- no target `kernel_forward`;
- no target `kernel_wake`;
- no target write tools;
- no time cursor;
- no sequence cursor;
- no trace page continuation;
- no visible `last_result_page`;
- no visible `last_result_partial`;
- no dynamic window target labels;
- no all_abouts/abouts scope policy;
- no raw access policy.

This dataset is excellent for the first read/navigation proof, but not enough
for full KMP operation.

## P0: Full KMP Operation Slice

The next P0 is not "run a bigger benchmark". It is:

```text
teach and verify the Operator can operate the KMP API surface, not only the
MemoryArena V6 subset.
```

### P0.1 Align Contract Validators

Implement one shared operator contract profile:

- exact action shape;
- exact tool schemas;
- full DimensionSelection semantics;
- boundedness;
- raw access policy;
- cursor mode rules;
- page continuation rules.

Acceptance:

- Rust validator catches every MCP/gRPC runtime semantic failure we can know
  statically;
- Python predictor validator matches Rust;
- fixtures cover valid/invalid cases for every tool;
- policy eval reports invalid reasons by category.

### P0.2 Add API-Conformance Trajectories

Create a small "KMP operator conformance" dataset before scaling benchmarks.

Implemented exporter:

```bash
cargo run -p kmp-testkit --bin kernel_operator_conformance_trajectory_export -- \
  --output /tmp/kernel-operator-conformance-full-v4 \
  --force

cargo run -p kmp-testkit --bin kernel_operator_contract_coverage -- \
  --profile full \
  --trajectories /tmp/kernel-operator-conformance-full-v4/trajectories.jsonl \
  --fail-under 100
```

Current generated suite:

| Metric | Value |
| --- | ---: |
| trajectories | 58 |
| read trajectories | 42 |
| write trajectories | 16 |
| target capability coverage | 100.00% |
| contract validation failures | 0 |

Current SFT preparation from that suite:

| Metric | Value |
| --- | ---: |
| selected rows | 58 |
| train rows | 44 |
| eval rows | 14 |
| dropped non-visible target refs | 0 |
| no-gold audit findings | 0 |
| model-facing full target capability coverage | 100.00% |
| `goal` included in model-facing prompt | yes |

Previous Qwen2.5-0.5B full-contract smoke:

| Metric | Value |
| --- | ---: |
| adapter | `/tmp/kernel-operator-qwen05-lora-conformance-full-v2` |
| predictions | `/tmp/kernel-operator-qwen05-conformance-full-v2-predictions` |
| strict policy eval | `/tmp/kernel-operator-qwen05-conformance-full-v2-policy-eval.json` |
| training epochs | 8 |
| final eval loss | 0.06752 |
| final eval mean token accuracy | 0.9894 |
| valid predictions | 25/30 |
| missing predictions | 5/30 |
| exact action accuracy | 6/30 |
| action type accuracy | 24/30 |
| tool accuracy | 17/27 tool calls |
| primary ref accuracy | 18/27 tool calls |
| scope accuracy | 23/27 tool calls |
| cursor mode accuracy | 4/13 cursor actions |
| window shape accuracy | 10/13 window actions |
| limit policy accuracy | 10/13 limit actions |
| continue page accuracy | 2/2 page continuations |

This is useful as a smoke test, not as a release metric. The model learned to
produce valid full-contract actions more often than the older pre-full adapter,
but the v2 30-row conformance set was only a coverage smoke. It was too small
to teach stable selection across temporal cursor modes, tool choice, dynamic
window policy, and strict smart-write details.

The v3 58-row run exposed a dataset-preparation gap: the SFT user prompt did
not include the top-level `goal`. The model saw visible state and allowed tools
but not the actual intent of the decision. Treat v3 predictions as diagnostic
only.

The v4 58-row run fixes that prompt shape and is the current conformance corpus
seed for training/evaluation. It is still a seed, not a final public dataset:
the next step is more variants per capability until strict-output failures go
to zero under the full contract.

Observed failure classes:

- `kernel_ask` sometimes used `dimensions.mode=only` without `include`;
- `kernel_write_memory` sometimes missed the top-level action type;
- `kernel_write_memory` sometimes added `semantic_delta` without
  `semantic_delta.why`;
- `kernel_write_memory` sometimes invented an extra `strategy` object;
- temporal move targets are still confused, especially `goto`/`rewind` being
  predicted as `forward`.

Generated paths:

```text
/tmp/kernel-operator-conformance-full-v4/trajectories.jsonl
/tmp/kernel-operator-conformance-full-v4-sft/openai_train.jsonl
/tmp/kernel-operator-conformance-full-v4-sft/openai_eval.jsonl
/tmp/kernel-operator-conformance-full-v4-sft/all_model_trajectories.jsonl
```

The write slice includes:

- rich `kernel_write_memory` with `chosen_because`, `why`, evidence, and
  read-context proof;
- rich `kernel_write_memory` with and without optional `semantic_delta`;
- anemic fallback `kernel_write_memory` with `follows`;
- fail-fast stop when a rich write lacks read-context proof;
- fail-fast stop when relation evidence is ambiguous;
- canonical `kernel_ingest` when a complete typed memory payload is ready;
- multi-entry canonical `kernel_ingest`.

Required cases:

| Case | Required target |
| --- | --- |
| Ask from current about | `kernel_ask` |
| Ask with conflicts | `kernel_ask(answer_policy=show_conflicts)` |
| Wake continuation | `kernel_wake` if included in read profile |
| Near by ref | `kernel_near(around.ref)` |
| Near by time | `kernel_near(around.time)` |
| Near by sequence | `kernel_near(around.sequence)` |
| Goto known-at | `kernel_goto(at.time/ref/sequence)` |
| Rewind previous decision | `kernel_rewind(from.ref/time/sequence)` |
| Forward later update | `kernel_forward(from.ref/time/sequence)` |
| Trace first page | `kernel_trace(page.entries=N)` |
| Trace continuation | `kernel_trace(page.cursor=next_cursor)` |
| Inspect light | `kernel_inspect(details=true, links=false)` |
| Inspect full typed | `kernel_inspect(details=true,incoming=true,outgoing=true,raw=false)` |
| Inspect raw audit | optional profile, `raw=true` |
| Stop sufficient evidence | `stop` |
| Stop unknown | `stop(answer_policy=evidence_or_unknown, final_refs=[])` |

### P0.3 Add Dynamic Window/Page Trajectories

Required policy labels:

- `expand_window`;
- `shrink_window`;
- `continue_page`;
- `switch_to_inspect`;
- `switch_to_trace`;
- `stop_sufficient`;
- `stop_budget`;
- `ask_instead_of_near`;
- `goto_known_at`;
- `forward_find_update`;
- `rewind_find_cause`.

Visible state must include:

- last tool;
- last observed refs;
- last observed count;
- last page:
  - returned;
  - total;
  - has_more;
  - next_cursor;
- last coverage:
  - included;
  - missing;
- last warnings;
- remaining tool calls;
- remaining context budget;
- evidence sufficiency label when generated by test harness;
- target rationale for audit, not model prompt, unless explicitly allowed.

### P0.4 Decide Profiles

Recommended profiles:

| Profile | Tools |
| --- | --- |
| `operator-read` | wake, ask, near, goto, rewind, forward, trace, inspect, stop |
| `operator-audit` | read profile + raw refs/raw inspect under explicit policy |
| `operator-write` | write_memory, possibly ingest, plus read_context proof tools |

Do not mix write into first release unless read profile is stable.

### P0.5 Replay Against Real MCP/API

Every accepted profile needs:

- offline strict policy eval;
- de-anonymized raw predictions;
- live MCP replay;
- zero MCP failures;
- zero missing expected refs;
- zero invalid predictions;
- zero unbounded calls;
- page continuation verified;
- temporal cursor modes verified.

## Acceptance Criteria For "Operator Knows KMP"

The phrase "Operator knows KMP" is allowed only when:

1. It covers every tool in the selected profile.
2. It covers every cursor mode in that profile.
3. It covers dimension scopes:
   - current_about;
   - abouts with non-empty list;
   - all_abouts explicit.
4. It covers dimension modes:
   - all;
   - only;
   - except.
5. It demonstrates dynamic window/page behavior.
6. It handles raw access policy explicitly.
7. It passes strict validator and live replay.
8. It reports all metrics by tool and by policy case.
9. It does not rely on benchmark gold fields.
10. It does not silently fallback.

Until then, the correct claim is narrower:

```text
Operator currently operates a strict bounded read/navigation subset of KMP,
validated on MemoryArena V6 holdout20 and live MCP replay.
```

The next public claim must be based on the capability-aware v7 read dataset plus
fresh training, strict policy eval, and live MCP replay.

## Immediate Work Items

P0 implementation checklist:

1. [x] Extend `kernel_operator_action_contract_error` with full dimension semantic
   validation.
2. [x] Add tests for invalid dimensions:
   - `mode=only` without include;
   - `mode=except` without exclude;
   - `scope=abouts` without abouts;
   - `scope=all_abouts` with abouts.
3. [x] Add cursor mode support to Operator validator:
   - ref;
   - time;
   - sequence.
4. [x] Add `kernel_write_memory` and `kernel_ingest` to the full Operator
   contract, including relation quality and read-context proof validation.
5. [x] Keep `operator-read` separate from write-capable `operator-full`.
6. [x] Add page-aware visible state to trajectory export from live/replay rows.
7. [x] Add evaluator metrics for dynamic window/page policy.
8. [x] Generate KMP conformance trajectories.
9. [x] Run a small conformance SFT/eval before any larger MemoryArena run.
   - [x] Prepare anonymized conformance SFT rows.
   - [x] Run no-gold audit on model-facing rows.
   - [x] Verify model-facing full target coverage is 100%.
   - [x] Train/predict in the GPU inference environment.
   - [x] Evaluate predictions with strict policy eval.
10. [x] Expand conformance data v4 before scaling:
   - add more rows per temporal cursor mode;
   - add more rows for dynamic window expansion/shrink;
   - add more trace continuation rows;
   - add more strict `kernel_write_memory` rows with and without
     `semantic_delta`;
   - add more canonical `kernel_ingest` rows.
11. [x] Grow conformance beyond coverage smoke enough for `operator-read`
    train/eval splits:
   - [x] at least two groups for every required read capability;
   - [x] duplicated `kernel_wake`;
   - [x] duplicated `dimensions.scope:all_abouts`;
   - [x] duplicated `trace.page:first`;
   - [x] capability-aware split preserves 100% read coverage in train and eval.
12. [ ] Grow conformance beyond coverage smoke for `operator-full`:
   - multiple variants per full target capability;
   - balanced temporal direction labels;
   - negative examples for invalid dimension modes;
   - strict smart-write examples without invented helper fields;
   - include the top-level `goal` in every model-facing prompt;
   - zero strict-output failures before live replay.
13. [ ] Replay clean conformance predictions through public TLS MCP endpoint.
14. [ ] Only after this, scale MemoryArena.
