# Operator Benchmark Status And Next Steps - 2026-05-14

Esta nota separa tres cosas que no deben mezclarse:

- calidad del Operator como modelo que decide acciones KMP/MCP;
- calidad del kernel como memoria navegable, temporal y multidimensional;
- calidad del sistema completo de QA multi-session, donde tambien intervienen el
  reader existente, plugins, reranker y razonamiento de tarea.

La conclusion corta:

```text
MemoryArena V6 valida muy bien la operacion de KMP/MCP.
LongMemEval multi-session sigue siendo el stress test pendiente del sistema
completo.
```

## Resultado Fuerte: Operator En MemoryArena V6

El resultado actual fuerte es el de MemoryArena V6 holdout20 con contrato
estricto.

Este benchmark mide si el modelo sabe operar herramientas KMP/MCP desde estado
visible:

- que herramienta llamar;
- con que referencia;
- con que scope;
- con que limites;
- cuando inspeccionar;
- cuando trazar;
- cuando moverse con `near`;
- cuando terminar con `stop`.

No mide respuesta generativa final ni razonamiento de dominio.

Artefactos principales:

| Artefacto | Path |
| --- | --- |
| Dataset eval | `/tmp/kernel-operator-sft-100-with-writer-by-task-anon-visible-candidate-details-holdout20/eval.jsonl` |
| Trajectories anonimizadas | `/tmp/kernel-operator-sft-100-with-writer-by-task-anon-visible-candidate-details-holdout20/eval_model_trajectories.jsonl` |
| Trajectories raw | `/tmp/kernel-operator-sft-100-with-writer-by-task-anon-visible-candidate-details-holdout20/eval_trajectories.jsonl` |
| Strict predictions | `/tmp/kernel-operator-qwen05-predictions-v6-holdout20-strict-20260514` |
| Raw strict predictions | `/tmp/kernel-operator-qwen05-predictions-v6-holdout20-strict-20260514-raw` |
| Policy eval anon | `/tmp/kernel-operator-qwen05-predictions-v6-holdout20-strict-20260514-policy-eval.json` |
| Policy eval raw | `/tmp/kernel-operator-qwen05-predictions-v6-holdout20-strict-20260514-raw-policy-eval.json` |
| Live MCP replay | `/tmp/kernel-operator-qwen05-predictions-v6-holdout20-strict-20260514-mcp-replay-full` |

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

Live MCP replay contra el kernel publico TLS:

| Metric | Value |
| --- | ---: |
| Selected steps | 1,124 |
| Executed MCP tool calls | 976 |
| Stop actions | 148 |
| Successful tool calls | 976 |
| Failed tool calls | 0 |
| Missing expected ref rows | 0 |
| Missing expected refs | 0 |
| Extra observed ref rows | 848 |
| Extra observed refs | 7,216 |
| Partial result rows | 424 |
| Elapsed | 10m15.9s |

Latencia del replay real:

| Action | Count | Avg ms | Max ms |
| --- | ---: | ---: | ---: |
| `kernel_near` | 424 | 1,302.7 | 2,517 |
| `kernel_inspect` | 424 | 110.0 | 198 |
| `kernel_trace` | 128 | 127.1 | 181 |
| `stop` | 148 | 0.0 | 0 |

Las 424 filas parciales vinieron de `kernel_near`. Eso es correcto: el resultado
esta acotado, la pagina es explicita y el replay registra `partial_result`
en vez de aceptar un recorrido sin limite.

## Ventana En La API Y En El Operator

La API ya permite controlar el tamano de lectura.

En la frontera KMP/MCP, las herramientas temporales exponen:

- `window.before_entries`;
- `window.after_entries`;
- `limit.entries`;
- `limit.tokens`;
- `budget.depth`;
- `budget.tokens`.

`trace` expone ademas:

- `page.entries`;
- `page.cursor`;
- `page.has_more`;
- `page.next_cursor`.

Por tanto, la capacidad existe en la API. No hay que cambiar el kernel para
pedir mas o menos contexto: el cliente puede hacer otra llamada con una ventana
o limite distinto, o continuar una pagina cuando `has_more=true`.

Lo que sabe hacer el Operator hoy:

| Caso | Window/limit emitido |
| --- | --- |
| `read` | `limit.entries=12`, `limit.tokens=2400`, `budget.depth=3`, `window.before_entries=6`, `window.after_entries=0` |
| `write_context_read` | `limit.entries=8`, `limit.tokens=1800`, `budget.depth=2`, `window.before_entries=3`, `window.after_entries=0` |

Eso demuestra que el Operator ya aprende a escoger ventanas acotadas distintas
segun el modo de uso.

Lo que todavia no queda demostrado, y pasa a ser el siguiente P0:

```text
adaptacion dinamica de ventana en varias llamadas:
si falta evidencia, ampliar;
si sobra contexto, reducir;
si page.has_more=true, seguir pagina;
si la evidencia ya basta, parar.
```

Para demostrar eso hacen falta trayectorias donde el target ensene esa politica
de forma explicita. El replay actual confirma que los resultados parciales se
ven y quedan registrados, pero no prueba todavia que el Operator haya aprendido
una politica general de expansion/reduccion de ventana.

## Gaps MCP/API Vs Operator

El contrato que consume el agente es KMP expuesto como herramientas MCP. El
Operator debe medirse contra esa frontera.

Hay tres niveles distintos:

| Nivel | Fuente |
| --- | --- |
| API canonica | `KernelMemoryService` gRPC en `api/proto/.../memory.proto` |
| Entrada agentica | MCP tools en `crates/kmp-mcp/src/protocol.rs` |
| Contrato del Operator | validador estricto en `crates/kmp-testkit/src/kernel_operator.rs` |

### Gap 1: Herramientas MCP Que El Operator No Cubre

MCP expone:

- `kernel_ingest`;
- `kernel_wake`;
- `kernel_ask`;
- `kernel_goto`;
- `kernel_near`;
- `kernel_rewind`;
- `kernel_forward`;
- `kernel_trace`;
- `kernel_inspect`.

El Operator actual es read/navigation-only y solo permite:

- `kernel_ask`;
- `kernel_goto`;
- `kernel_near`;
- `kernel_rewind`;
- `kernel_forward`;
- `kernel_trace`;
- `kernel_inspect`;
- `stop`.

No cubre:

- `kernel_ingest`;
- `kernel_write_memory`;
- `kernel_wake`.

Decision actual: no es bug para el modelo de lectura, pero si es gap de producto
si queremos que Operator opere todo KMP/MCP.

### Gap 2: Herramientas Permitidas Pero No Entrenadas En El Holdout Actual

En MemoryArena V6 holdout20, el target real uso solo:

| Tool | Count |
| --- | ---: |
| `kernel_near` | 424 |
| `kernel_inspect` | 424 |
| `kernel_trace` | 128 |
| `stop` | 148 |

Aunque `allowed_tools` incluia tambien `kernel_ask`, `kernel_goto`,
`kernel_rewind` y `kernel_forward`, no aparecieron como target en ese holdout.

Por tanto, el score `1.000` no demuestra dominio de:

- `kernel_ask`;
- `kernel_goto`;
- `kernel_rewind`;
- `kernel_forward`;
- cursores por tiempo;
- cursores por secuencia.

### Cierre 3: Cursor API En El Operator

MCP/API acepta cursor temporal por:

- `ref`;
- `time`;
- `sequence`.

El contrato del Operator acepta ya los tres modos:

```text
around.ref
around.time
around.sequence
at.ref
at.time
at.sequence
from.ref
from.time
from.sequence
```

MemoryArena V6 no ejercita `time` ni `sequence`, asi que su score no demuestra
dominio de esos cursores. La suite de conformance v4 si los cubre como parte
del perfil full.

### Gap 4: Operator Es Mas Estricto Que MCP En Campos Opcionales

MCP/API permite defaults en varios campos:

- `answer_policy` opcional en `kernel_ask`;
- `dimensions` opcional;
- `window` opcional;
- `limit` opcional;
- `include` opcional;
- `budget` opcional;
- `trace.page` opcional;
- `inspect.include` opcional.

El Operator exige muchos de esos campos:

- `kernel_ask`: exige `answer_policy` y `dimensions`;
- temporal tools: exige `dimensions`, `include`, `limit`, `budget`, `window`;
- `kernel_trace`: exige `budget`;
- `kernel_inspect`: exige `include`.

Esto no es necesariamente malo. Para un Operator publicable, una salida mas
explicita y acotada es mejor que depender de defaults. Pero hay que llamarlo por
su nombre:

```text
El Operator opera un subconjunto estricto y acotado de MCP/API.
No opera todavia todo el espacio valido de la API.
```

### Cierre 5: Reglas De Dimensiones En El Operator

El mapping MCP/gRPC valida reglas semanticas:

- `mode=all` no debe llevar `include` ni `exclude`;
- `mode=only` requiere `include`;
- `mode=except` requiere `exclude`;
- `scope=current_about` no debe llevar `abouts`;
- `scope=abouts` requiere `abouts` no vacio;
- `scope=all_abouts` no debe llevar `abouts`.

El validador del Operator replica ya estas reglas semanticas. Eso evita que una
accion pase el policy eval del Operator y falle despues en MCP/gRPC por una
seleccion de dimensiones invalida.

La regla de producto queda asi:

```text
Una salida del Operator debe ser mas estricta que MCP si hace falta, pero no
debe aceptar nada que MCP/gRPC vaya a rechazar de forma predecible.
```

### Gap 6: `inspect.raw=true` Existe En MCP, Pero Operator Lo Bloquea

MCP/API permite:

```json
{"include": {"raw": true}}
```

El Operator actual exige `include.raw=false`.

Esto fue una decision conservadora por seguridad y estabilidad de shape. No es
un bug del benchmark, pero si limita al Operator como usuario completo de la
API. Si queremos que pueda auditar raw refs, hay que entrenarlo y evaluarlo de
forma explicita.

### Gap 7: Page Metadata Llega En Replay, Pero No En El Dataset V6

El replay real registro `partial_result=true` y `page` en resultados MCP.

Pero el dataset V6 usado para entrenar/evaluar este Operator no contiene
`visible_state.last_result_page` ni `visible_state.last_result_partial`. En el
holdout actual:

```text
last_result_page rows = 0
last_result_partial rows = 0
```

Por tanto, el Operator no pudo aprender politica de continuacion de pagina desde
ese dataset.

Este es el gap principal del siguiente P0:

```text
capturar page metadata en trayectorias y entrenar al Operator a usarla.
```

### Gap 8: Politica Dinamica De Ventana No Esta Etiquetada

El Operator aprendio dos ventanas fijas:

| Modo | Window/limit |
| --- | --- |
| `read` | `limit.entries=12`, `limit.tokens=2400`, `budget.depth=3`, `window.before_entries=6`, `window.after_entries=0` |
| `write_context_read` | `limit.entries=8`, `limit.tokens=1800`, `budget.depth=2`, `window.before_entries=3`, `window.after_entries=0` |

Eso demuestra seleccion de preset por modo, no una politica dinamica.

Falta etiquetar y medir:

- expandir ventana;
- reducir ventana;
- continuar pagina;
- parar por evidencia suficiente;
- cambiar de `near` a `inspect`;
- cambiar de `near` a `trace`.

## P0: Cobertura 100% Del Perfil MCP/API

Si el Operator se coloca entre un LLM y KMP/MCP, el contrato del Operator no
puede ser mas pobre que la API real sin declararlo como perfil limitado.

El riesgo es:

```text
KMP/MCP expone una capacidad.
El LLM solo opera a traves del Operator.
El Operator no conoce esa capacidad.
La API efectiva queda mutilada.
```

Por tanto, antes de poner el Operator como puerta principal de KMP, necesitamos
medir cobertura de contrato y cobertura de datos.

Objetivo para cualquier perfil publicable:

| Cobertura | Objetivo |
| --- | ---: |
| Contract coverage del perfil | 100% |
| Dataset target coverage del perfil | 100% |
| Live replay coverage del perfil | 100% |

Esto exige datasets por caso de uso de la API/MCP, no solo mas ejemplos del
mismo benchmark.

Suites P0 necesarias:

| Suite | Cubre |
| --- | --- |
| `kmp-read-wake` | `kernel_wake` |
| `kmp-read-ask` | `kernel_ask` y `answer_policy` |
| `kmp-temporal-ref` | cursores por `ref` |
| `kmp-temporal-time` | cursores por `time` |
| `kmp-temporal-sequence` | cursores por `sequence` |
| `kmp-dimensions-mode` | `all`, `only`, `except` |
| `kmp-dimensions-scope` | `current_about`, `abouts`, `all_abouts` |
| `kmp-trace-pagination` | `page.entries`, `page.cursor`, continuacion |
| `kmp-window-policy` | ampliar, reducir, continuar, parar |
| `kmp-inspect-policy` | inspect ligero/completo con `raw=false` |
| `kmp-audit-raw` | raw refs/raw inspect si existe perfil auditor |
| `kmp-write-memory` | escritura semantica con relacion, why, evidencia y read_context |
| `kmp-ingest-canonical` | ingest canonico si entra en el perfil full |

Cada suite debe producir targets estrictos, policy eval offline y replay real
contra MCP/gRPC. Si falta una suite, la cobertura debe bajar y hacerlo visible.

Medicion actual con `kernel_operator_contract_coverage` sobre el holdout V6:

| Metric | Value |
| --- | ---: |
| MCP global tool coverage from `operator-read` | 80.00% |
| MCP global tool coverage from `operator-full` | 100.00% |
| Read profile contract coverage | 100.00% |
| Full profile contract coverage | 100.00% |
| V6 target capability coverage | 41.67% |
| V6 target capability coverage against full profile | 35.71% |
| KMP conformance full target capability coverage | 100.00% |
| P1.11 + conformance v7 read train target capability coverage | 100.00% |
| P1.11 + conformance v7 read eval target capability coverage | 100.00% |
| KMP conformance SFT no-gold audit findings | 0 |
| KMP conformance v7 selected rows | 61 |
| KMP conformance v7 read/write rows | 45 / 16 |
| KMP conformance v7 contract validation failures | 0 |
| KMP conformance v7 model-facing read train/eval target coverage | 100.00% / 100.00% |
| KMP conformance v7 prompt includes `goal` | yes |

Interpretacion:

- el contrato lector ya puede expresar el perfil `operator-read`;
- el contrato full ya puede expresar escritura canonica y escritura helper;
- el dataset V6 no cubria ni todo read ni full;
- la mezcla P1.11 + conformance v7 ya cubre el perfil `operator-read` en train
  y eval; todavia falta entrenar, evaluar y replayar el modelo nuevo antes de
  convertirlo en claim publico;
- la suite sintetica de conformance v7 cubre todo el contrato full y ya es el
  seed correcto para entrenamiento de conformidad; para `operator-full` sigue
  siendo pequena y hay que ampliar variantes por capacidad antes de considerarla
  un corpus estable;
- el SFT model-facing generado desde conformance no filtra campos gold y no
  tiene targets con refs no visibles;
- el SFT v4+ corrige el gap de prompt detectado en v3: el modelo recibe el
  `goal` de la decision, no solo el estado visible y las herramientas
  permitidas;
- los resultados v2/v3 son diagnosticos. V2 era demasiado pequeno; v3 expuso
  que faltaba `goal` en el prompt SFT. Los resultados de modelo deben tomarse
  de v4 o posterior; el proximo intento publicable debe usar el split
  capability-aware v7.
- `kernel_ingest` y `kernel_write_memory` quedan fuera del perfil lector y por
  eso el coverage global MCP desde `operator-read` no es 100%.

## P0 Derivado De Estos Gaps

Antes de escalar benchmark o publicar Operator:

1. Generar trayectorias page-aware con `last_result_page` y
   `last_result_partial`.
2. Crear targets que ensenen:
   - continuar pagina;
   - ampliar ventana;
   - reducir ventana;
   - parar cuando hay evidencia suficiente.
3. Incluir cobertura real de:
   - `kernel_ask`;
   - `kernel_goto`;
   - `kernel_rewind`;
   - `kernel_forward`;
   - cursor `time`;
   - cursor `sequence`.
4. Crear datasets por caso de uso MCP/API hasta cubrir el 100% del perfil
   declarado y, despues, ampliar cada caso con suficientes variantes para que
   el modelo aprenda politica, no solo cobertura.
5. Medir `operator_contract_coverage`, `dataset_target_coverage` y
   `live_replay_coverage` con umbral 100% para el perfil publicable.
6. Mantener escritura fuera del primer entrenamiento publicable. El primer
   perfil debe ser `operator-read`; `kernel_write_memory` y `kernel_ingest`
   quedan para un writer separado.

## Decision Sobre Escritura

La escritura no entra en el siguiente entrenamiento como `operator-full`.

La auditoria de conformance mostro que algunas muestras de escritura estaban
forzando al modelo a inventar contenido. Eso ya se corrigio para que el corpus
sea honesto, pero el problema de producto es mas profundo: escribir memoria KMP
no es solo emitir un JSON grande.

El flujo correcto de escritura es:

```text
leer contexto -> inspeccionar nodos -> decidir relacion -> escribir o fallback
```

El `why` de una relacion rica no es determinista. Lo decide el writer/LLM que
usa KMP despues de leer contexto suficiente. El kernel no infiere ese
significado; lo valida. Si el writer no puede justificar una relacion rica,
debe terminar en la relacion anemica determinista por defecto, normalmente
`follows`.

Por tanto:

- `operator-read` sigue siendo P0;
- escritura queda como diseno separado de writer inteligente;
- los casos de `kernel_write_memory`/`kernel_ingest` se conservan como tests de
  contrato, anti-invencion y futura base de writer;
- no se debe publicar un claim de `operator-full` hasta tener el flujo de
  escritura con lectura previa, decision semantica y fallback anemico medido.

La decision semantica rica de writer no debe cargarse inicialmente sobre un
modelo 0.5B. El 0.5B debe ser Operator en sentido estricto: solo sabe usar el
kernel. Su trabajo es mover la lectura, acotar herramientas, detectar si falta
contexto, ejecutar escrituras preparadas y escalar cuando la relacion requiere
juicio semantico. Para generar datasets de writer, el teacher preferente es
GPT-5.5 en modo offline, con salida estructurada, refs citadas y procedencia
guardada por muestra.

Regla de dataset:

```text
GPT-5.5 teacher decide relacion/why/evidencia.
Operator 0.5B aprende solo a usar KMP y a escalar.
Kernel valida; no infiere.
```

Si GPT-5.5 no esta disponible para ese corte, la generacion de muestras writer
falla. No se sustituye silenciosamente por otro modelo.

## Que Demuestra Este Resultado

Este resultado demuestra que:

- el Operator puede aprender la politica de navegacion KMP/MCP en este corpus;
- las acciones emitidas cumplen el contrato estricto;
- no se estan aceptando campos extra ni acciones parecidas al contrato;
- no hay llamadas no acotadas;
- las refs anonimizadas pueden mapearse de vuelta a refs reales;
- las acciones funcionan contra el kernel desplegado, no solo contra JSON local;
- la paridad practica MCP/gRPC aguanta el replay real;
- la paginacion aparece de forma visible en resultados parciales.

Para claims externos, esta es una evidencia fuerte de tool-operation.

## Que No Demuestra

Este resultado no demuestra que:

- el sistema completo resuelva QA multi-session;
- el reader interprete siempre bien evidencia recuperada;
- el modelo haga razonamiento numerico, temporal o de preferencias;
- KMP sea un generador de respuestas;
- el Operator sustituya a un LLM grande;
- MemoryArena V6 holdout20 baste como release publico final.

La frase correcta para producto:

```text
Operator aprende a operar memoria KMP de forma acotada y reproducible.
No sustituye al reader existente ni al razonador generativo.
```

## Gap Real: LongMemEval Multi-Session

LongMemEval mide otra cosa. No basta con saber moverse por la memoria. El
sistema tiene que leer conversaciones de varias sesiones, elegir evidencia,
resolver conflictos, aplicar actualizaciones, entender preferencias y producir
una respuesta final.

En los experimentos anteriores, el sistema mostro una separacion importante:

- el kernel podia recuperar evidencia relevante con mucha fuerza;
- el end-to-end QA quedaba bastante por debajo;
- el reader actual fallaba en casos donde tenia que elegir entre respuestas
  plausibles o interpretar operaciones de dominio;
- algunas preguntas requerian sumar, deduplicar, elegir el valor mas reciente,
  comparar fechas o entender estados como pagado, planificado o cancelado.

Resultado historico orientativo:

| Slice | Lectura |
| --- | --- |
| LongMemEval multi-session QA | aproximadamente `0.7174` end-to-end en el corte medido |
| LongMemEval v8 clean Operator | `0.7500` exact action accuracy, experimento interno |
| LongMemEval v10 operator-state | `0.9833` exact action accuracy, 1 fallo contractual aislado |

La lectura correcta:

```text
LongMemEval no invalida KMP.
LongMemEval muestra que QA multi-session necesita mas que recuperacion:
necesita reader, plugins, reranking y una politica de interpretacion.
```

## Rectificacion Importante: El Reader Ya Existe

El reader no parte de cero.

Ya tenemos piezas de reader y de interpretacion:

- `ComposedEvidenceReader::kernel_default()`;
- `EvidenceReaderPluginConfigurator`;
- `memoryarena_kmp_reader`;
- `memoryarena_kmp_plugin_reader`;
- integracion del plugin reader en `longmemeval_kmp_runner`;
- plugins base de valores y derivaciones ya implementados para codigo,
  matematicas, URLs, dinero, fechas y operaciones deterministas.

Por tanto, el siguiente trabajo no es "crear un reader". La formulacion correcta
es:

```text
usar, endurecer, configurar y medir el reader existente dentro del pipeline
LongMemEval/MemoryArena.
```

El gap real no es "no tenemos plugins". El gap real esta en:

- asegurar que el benchmark usa el reader/plugin reader correcto;
- separar metricas de recuperacion, operator, reader y plugins;
- hacer que la seleccion de operandos quede trazable;
- medir cuando el reader ignora evidencia correcta;
- evitar que el kernel core absorba operaciones de dominio que pertenecen al
  reader o a plugins.

## Problemas Detectados En LongMemEval

### 1. Dataset y session ids

La version cleaned 500 full-history contiene casos con `session_id` repetidos o
colisionados dentro de una misma pregunta. El adapter antiguo podia terminar
generando dimensiones duplicadas.

Decision actual:

```text
No hay fallback.
Si el shape del dataset no esta soportado, el adapter falla rapido.
```

Esto protege KMP de inventar identidades silenciosas.

### 2. QA multi-session no es lo mismo que tool-operation

MemoryArena V6 pregunta si el Operator llama bien a herramientas. LongMemEval
pregunta si el sistema completo responde bien.

Para LongMemEval hacen falta piezas adicionales:

- retrieval candidato;
- reranking;
- reader existente configurado para el benchmark;
- plugins existentes de fechas, dinero, codigo, URLs, matematicas y operaciones,
  mas los gaps concretos de conteo/dedupe/latest si el caso los exige;
- reglas de actualizacion de conocimiento;
- criterio para elegir evidencia final;
- judge reproducible.

### 3. Relaciones y paths no resuelven todo

Las relaciones ricas son utiles para causalidad, dependencia, sustitucion,
contradiccion, refinamiento y explicacion de proceso.

No son suficientes por si solas para preguntas agregadas como:

- cuanto dinero total;
- cuantos elementos;
- cual es el ultimo valor vigente;
- que evento cancela a otro;
- que fecha domina;
- que preferencias sobreviven a cambios posteriores.

Esas operaciones pertenecen a plugins/readers por encima de KMP, no al core.

### 4. El reader es parte del resultado

Si el kernel recupera la evidencia correcta pero el reader la interpreta mal,
el fallo no debe clasificarse como fallo de memoria.

Clasificacion necesaria:

| Clase | Significado |
| --- | --- |
| Ingesta | La memoria escrita no contenia lo necesario. |
| Proyeccion | La memoria escrita no llego al read model. |
| Recuperacion | La consulta no encontro la evidencia. |
| Prueba | La evidencia no quedo trazable o inspeccionable. |
| Reader | La evidencia estaba, pero el lector no la uso bien. |
| Razonamiento de tarea | La respuesta exigia operaciones fuera de recuperacion. |

## Estado Del Operator Tras La Auditoria De Contrato

El bug encontrado no era del kernel. Era de medicion del Operator.

Antes, predictor/evaluator aceptaban acciones parecidas al contrato. Ejemplo:
un `kernel_ask.arguments.final_refs` era invalido, porque `final_refs` solo
pertenece a `stop`.

Estado actual:

- validador compartido en Rust testkit;
- predictor Python endurecido;
- evaluator endurecido;
- no additional properties;
- versionado `kernel-operator-action-contract-v1`;
- `schema_mode = strict-no-additional-properties`;
- acciones invalidas van a `failures.jsonl`, no a `predictions.jsonl`;
- los resultados pre-strict no son publicables salvo revalidacion.

Esto sube la barra correcta:

```text
El Operator no emite texto.
Emite decisiones operacionales ejecutables.
```

## Decision De Producto

La linea publica debe ser MemoryArena-first para Operator.

Claim permitido:

```text
Operator aprende a elegir acciones KMP/MCP acotadas desde estado visible de
memoria y esas acciones se reproducen contra un KMP real.
```

Claim no permitido:

```text
Operator resuelve memoria multi-session end-to-end.
```

LongMemEval queda como regresion secundaria del sistema completo. Es valiosa,
pero no debe dirigir el primer release del Operator hasta que:

- el adapter soporte correctamente sus shapes;
- la ingesta sea limpia y fail-fast;
- el reader existente se ejecute con plugins/reranking donde aplique;
- el benchmark quede reproducible sin usar campos gold;
- el score se pueda explicar por clases de fallo.

## Siguientes Pasos

### P0 - Politica Dinamica De Ventana Y Paginacion

Este es el siguiente corte antes de escalar benchmark.

Objetivo:

```text
El Operator debe aprender a regular el tamano de contexto usando la API KMP/MCP:
ampliar, reducir, continuar pagina o parar.
```

1. Crear o exportar trayectorias especificas donde el target obligue a decidir:
   - ampliar ventana cuando falta evidencia;
   - reducir ventana cuando el contexto recuperado es excesivo;
   - continuar con `page.next_cursor` cuando `page.has_more=true`;
   - cambiar de `near` a `inspect` cuando ya hay candidato concreto;
   - cambiar de `near` a `trace` cuando hay que justificar camino;
   - parar cuando la evidencia observada ya es suficiente.
2. Incluir en `visible_state` los datos necesarios para esa decision:
   - `last_result_partial`;
   - `last_result_page.has_more`;
   - `last_result_page.next_cursor`;
   - refs observadas;
   - refs esperadas o suficientes si estan visibles por la trayectoria;
   - presupuesto restante;
   - numero de nodos/candidatos leidos;
   - motivo de la siguiente accion target.
3. Ampliar el evaluator con metricas especificas:
   - `expand_window_accuracy`;
   - `shrink_window_accuracy`;
   - `continue_page_accuracy`;
   - `stop_when_sufficient_accuracy`;
   - `over_read_rate`;
   - `under_read_rate`;
   - `unnecessary_page_continue_rate`.
4. Mantener el contrato fail-fast:
   - `invalid_predictions > 0`;
   - `unbounded_tool_calls > 0`;
   - refs no visibles;
   - leakage de target/gold;
   - replay MCP con failures;
   - missing expected refs.
5. Ejecutar replay MCP real con casos donde:
   - `near` devuelve `partial_result=true`;
   - `trace` requiere al menos dos paginas;
   - una segunda llamada reduce ventana o cambia de herramienta;
   - el Operator para sin leer de mas.
6. Documentar latencia y tokens/contexto consumido por politica:
   - ventana inicial;
   - ventana ampliada;
   - ventana reducida;
   - pagina continuada;
   - stop.

Criterio de aceptacion P0:

```text
El Operator no solo emite ventanas acotadas.
El Operator demuestra una politica observable para cambiar la ventana o pagina
segun lo que devolvio la API.
```

### P0 Despues - Cerrar Base Publicable Del Operator

1. Mantener MemoryArena V6 holdout20 como evidencia fuerte, pero no como release
   final.
2. Ejecutar un MemoryArena mas grande y fresco solo despues del P0 dinamico, con
   el mismo pipeline:
   - smart writer;
   - refs anonimizadas;
   - split agrupado;
   - strict predictor;
   - strict policy eval;
   - de-anon raw;
   - live MCP replay;
   - metricas de expansion/reduccion/paginacion.
3. Preparar model card y dataset card solo con resultados strict.
4. Publicar primero privado en Hugging Face, descargar de nuevo y repetir replay.

### P1 - Recuperar LongMemEval Bien

1. No usar LongMemEval cleaned 500 full-history para claims hasta resolver
   `session_id` repetidos.
2. Definir una semantica explicita para sesiones repetidas:
   - misma sesion logica;
   - fragmento de sesion;
   - version de sesion;
   - o dimension compuesta con indice estable.
3. Ejecutar un slice pequeno de LongMemEval con dataset/adapters limpios.
4. Separar metricas:
   - retrieval evidence recall;
   - Operator tool-operation;
   - reader answer accuracy;
   - plugin/operator accuracy;
   - judge accuracy.
5. Repetir el reader con los plugins existentes activados y reportados:
   - fechas;
   - dinero/currency;
   - codigo fuente;
   - URLs;
   - matematicas;
   - operaciones deterministas.
6. Documentar como gaps explicitos los plugins o politicas que falten para el
   benchmark:
   - conteo/dedupe;
   - latest/current;
   - canonicalizacion de entidades;
   - preferencias/estado vigente.
7. Medir fallos por clase, no solo score global.

### P1 - Reader Existente Y Plugins

1. Mantener plugins fuera del core KMP.
2. Usar la arquitectura de plugins existente:
   - `ComposedEvidenceReader`;
   - `EvidenceReaderPluginConfigurator`;
   - `SourceCodeValuePlugin`;
   - `MathExpressionValuePlugin`;
   - `UrlValuePlugin`;
   - `MoneyValuePlugin`;
   - `DateValuePlugin`;
   - `ValueOperationPlugin`;
   - `CurrencyDerivationPlugin`;
   - `DateDerivationPlugin`.
3. Endurecer el reader existente por composicion:
   - lista ordenada de plugins;
   - configuracion explicita;
   - salida trazable por plugin;
   - fail-fast si un plugin requerido no esta disponible.
4. Prioridad real:
   - confirmar que los plugins implementados estan conectados en cada runner;
   - medir impacto con/sin plugin reader;
   - anadir solo los plugins que falten por fallo observado, no por intuicion.
5. Gaps probables, si LongMemEval los exige:
   - conteo con dedupe;
   - latest/current;
   - canonicalizacion de entidades;
   - preferencias/estado vigente.
6. Medir:
   - evidencia recuperada;
   - valores extraidos;
   - operaciones aplicadas;
   - respuesta final.
7. Confirmar en cada benchmark que camino de reader se uso:
   - reader determinista;
   - reader LLM;
   - `ComposedEvidenceReader`;
   - plugin reader;
   - combinacion reader + plugins + derivaciones.

### P1 - Operador Mas Robusto

1. Mantener Qwen 0.5B como baseline actual fuerte.
2. No descartar otros modelos, pero tratarlos como controles:
   - Hammer 0.5B;
   - FunctionGemma 270M;
   - LFM2.5 Nova;
   - modelos NVIDIA si el stack CUDA/modelo es viable.
3. Priorizar constrained emission o schema-guided decoding:
   - una sola accion;
   - enum de tools;
   - argumentos cerrados por tool;
   - sin campos extra;
   - limites obligatorios.
4. Entrenar con ejemplos negativos de contrato:
   - `final_refs` dentro de tool call;
   - `kernel_near` sin `limit`;
   - `kernel_trace` sin page;
   - refs no visibles;
   - acciones duplicadas en una respuesta.

### P2 - Escalabilidad Y Observabilidad

1. Mantener page metadata como requisito de primer nivel.
2. Asegurar que `trace`, `near`, `goto`, `rewind` y `forward` no materializan
   grafos completos sin limite.
3. Medir latencias por accion en cada replay.
4. Seguir especialmente `kernel_near`, porque domina latencia.
5. Emitir logs entendibles para humanos:
   - cuantos nodos leyo;
   - cuantos candidatos vio;
   - que pagina devolvio;
   - que ref eligio;
   - por que paro.
6. Integrar OTel/metricas:
   - latencia por herramienta;
   - projection lag;
   - page size;
   - partial results;
   - missing refs;
   - relation quality;
   - reader ignored evidence.

### P2 - Modelo Operador Especializado

La idea de entrenar un modelo pequeno para operar KMP sigue siendo razonable.
No debe ser un agente general. Debe ser un especialista estrecho:

```text
Estado visible -> siguiente accion KMP/MCP acotada.
```

Debe aprender:

- cuando hacer `near`;
- cuando hacer `trace`;
- cuando hacer `inspect`;
- cuando parar;
- cuando pedir pagina siguiente;
- cuando el contexto ya es suficiente;
- cuando escalar a un LLM grande.

Debe escalar cuando:

- hay ambiguedad semantica no resoluble con herramientas;
- la evidencia es contradictoria;
- necesita razonamiento de dominio;
- necesita redactar respuesta final;
- necesita crear relaciones nuevas con significado rico.

## Orden Recomendado Desde Aqui

1. Commit del corte strict/replay/documentacion.
2. Implementar P0 de politica dinamica de ventana/paginacion.
3. Reentrenar/evaluar el Operator con trayectorias que ensenen ampliar,
   reducir, continuar pagina y parar.
4. Solo despues, ejecutar MemoryArena mayor con el pipeline validado.
5. Preparar assets de Hugging Face privados para Operator.
6. Reabrir LongMemEval como regresion secundaria, no como claim principal.
7. Conectar y medir el reader composable existente con los plugins ya
   implementados en el benchmark.
8. Repetir LongMemEval pequeno con reader/plugin reader y clasificacion de
   fallo.
9. Solo despues, plantear LongMemEval 500 como resultado publicable.

## Frase Para Mantener En La Cabeza

```text
KMP no promete que cualquier reader razone bien.
KMP promete memoria navegable, acotada, auditable y reproducible.
Operator aprende a operar esa memoria.
Reader y plugins convierten evidencia en respuesta.
```
