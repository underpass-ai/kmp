# Actualizar la superficie de KMP hacia el agente

Este procedimiento sirve a quien mantiene KMP, sea una persona o un agente.
La superficie incluye lo que el agente recibe al conectarse, las herramientas
que descubre, las skills que invoca, la guía que consulta y las respuestas
con las que decide el siguiente movimiento. Cambiar una de esas partes puede
exigir actualizar las demás.

El resultado debe permitir usar la capacidad con evidencia y un coste de
contexto conocido. Una descripción más corta no es una mejora si provoca
llamadas incorrectas, pierde prueba o confunde los relojes.

## Dónde se cambia cada cosa

Las rutas son relativas a la raíz del repositorio.

| Qué cambia | Fuente que se edita | Qué debe acompañarlo |
| --- | --- | --- |
| Comportamiento del núcleo | Dominio, casos de uso y adaptadores que implementan la operación | Prueba de comportamiento y contrato MCP correspondiente |
| Argumentos, resultados y descripción de una herramienta | [contract/tools](../../crates/kmp-mcp/src/contract/tools/) y [contract/schema](../../crates/kmp-mcp/src/contract/schema/); registro en [registry.rs](../../crates/kmp-mcp/src/contract/registry.rs) | Fixture de superficie, guía del verbo y ejemplo afectado |
| Reglas comunes al conectar un agente | [agent_policy/instructions.rs](../../crates/kmp-mcp/src/agent_policy/instructions.rs), servido por [handshake.rs](../../crates/kmp-mcp/src/contract/handshake.rs) | Casos de routing y captura de initialize y del catálogo que expone el host |
| Vocabulario de relaciones | `KnownMemoryRelationType` y su especificación en [relation_type.rs](../../crates/kmp-domain/src/value_objects/relation_type.rs) | Proyección [relation_vocabulary.rs](../../crates/kmp-mcp/src/contract/schema/relation_vocabulary.rs), casos positivos/negativos y enseñanza de why/evidence |
| Entrada breve del agente | [guide/agent-entry.md](../../plugins/kmp/guide/agent-entry.md) | Regenerar el índice y comprobar su tamaño; no insertar el manual completo |
| Explicación extendida o ejemplo | [editorial.json](../../plugins/kmp/guide/editorial.json) y sus `text_file` en `verbs/`, `topics/` o `examples/` | Referencia válida, prueba nativa y lectura selectiva |
| Correspondencia entre herramienta y verbo | `tool_verb` en [GuideRequestMapper](../../crates/kmp-release/src/application/mappers/guide_request_mapper.rs) | Todo tool público debe resolver a un verbo indexado |
| Ayuda ante un rechazo de uso | [tool_error_help.rs](../../crates/kmp-mcp/src/serving/tool_error_help.rs) y su envoltorio en `tool_result.rs` | Elegir por código/ruta tipados, conservar feedback y ejecutar las lecturas sugeridas |
| Cómo se entra desde Codex o Claude | [skills](../../plugins/kmp/skills/), [adaptadores Claude](../../plugins/kmp/claude/commands/) y [capabilities.json](../../plugins/kmp/capabilities.json) | Paridad de capacidades y acceso a la misma entrada; evitar otra copia del manual |
| Relato público del producto | Bloque `kmp:public-overview` de [plugins/kmp/README.md](../../plugins/kmp/README.md) | Sincronizar README de repositorio y crate; revisar guía humana |

`guide/AGENT.md`, `guide/guide.requests.json` y `guide/memory.jsonl` son salidas
generadas. No corregirlas a mano: corregir la fuente y regenerar las tres.
Los fixtures de contrato son evidencia revisable; no se actualizan sólo para
hacer pasar un test que ha detectado un cambio inesperado.

## 1. Definir el cambio desde el uso

Describir el caso que cambia y el resultado observable antes/después. Identificar
qué debe aprender el agente, qué argumento utiliza, qué devuelve KMP y qué
prueba permite confiar en ello. Incluir la ruta temporal y relacional cuando
la tarea requiere recorrer una historia; un ejemplo de Ask no cubre ese uso.

Anotar las superficies afectadas con la tabla anterior. Si sólo cambia una
explicación, el comportamiento del núcleo no necesita cambiar. Si se añade
un tool, revisar también inventario, registro, mapping de guía, fixtures,
`distribution/mcpb/manifest.json` y el descubrimiento real en el host.
El manifiesto enumera las herramientas del paquete que instala el host;
comprobarlo con `bash scripts/ci/mcp-registry.sh`. Evitar repetir un recuento fijo en varios tests.

## 2. Colocar la explicación donde se consulta

Mantener la entrada breve como mapa. El detalle de un verbo debe explicar
cuándo usarlo, cuándo elegir otro, entrada mínima, resultado, errores o límites
y siguiente movimiento. Las skills dirigen a esa entrada y reutilizan lo ya
leído; no vuelven a cargar toda la guía para cada operación.

En `editorial.json`, cada entrada tiene un `id` estable y **una** fuente:
`text` o `text_file`. `guide_title` la incluye como verbo/tema en el índice;
`example_title` la incluye como lección. El builder conserva el cuerpo como
nodo KMP y genera sus referencias completas. El lector copia esas referencias,
no las reconstruye. Mantener el `id` de un concepto existente y regenerar su
contenido evita romper referencias por un cambio de redacción.

Un ejemplo nuevo debe contener fuentes explícitas, decisiones de escritura
justificadas, llamadas con refs devueltas, resultado esperado y límites. Separar
fuentes disponibles de preguntas futuras. Incluir un contraejemplo cuando
haya una confusión probable: compartir etiqueta no prueba identidad; ocurrido
ayer no significa observado ayer. Un replay escrito demuestra un contrato;
no demuestra que otro LLM haya aprendido a usarlo.

En escritura, consumir toda la lista `feedback`: cada registro que falla durante
la compilación independiente aporta su primer error, con código, ruta y acción
si existe. Conservar esos campos al agrupar; no reconstruir reparaciones desde
el mensaje general. La forma del paquete, sus identidades y los fallos del
backend pueden detener la validación antes. Tampoco es una lista exhaustiva de
todos los errores de un mismo registro. Comprobar que el rechazo completo no
escribe nada y que las acciones siguen siendo ejecutables después de agrupar.

La autoverificación del escritor debe comparar fuentes con lo que realmente
guardó: reloj de observación frente a ocurrencia, vigencia, prueba y una ruta
dimensional útil. Reutilizar el contexto ya disponible y ampliar sólo lo necesario.
La cobertura del paquete no demuestra cobertura de la fuente; el lint literal
de `summary_en` no comprueba equivalencias semánticas ni fechas dentro de la
prosa. Enseñar ejemplos concretos de esas diferencias, claves estables para una
misma ruta y hechos con ciclos de vida separados. No imponer cuotas de tipos o
etiquetas ni convertir esta revisión informativa en un gate editorial.

Al cambiar la representación de una continuación, comprobar la reconstrucción
completa desde sus páginas contra una lectura sin paginar. En Inspect,
`page.repeat_object=false` sólo sirve con un cursor y el objeto inicial
conservado; `object_reused=true` y `object.ref` identifican lo reutilizado.
Texto, metadatos, fuente y pruebas siguen vinculados al cursor, aunque no se
repitan. Inspect devuelve la llamada completa en `next_actions`; si no cabe un
elemento, su presupuesto ofrece al menos `page.minimum_progress_bytes`; prefiere
la inspección completa cuando cabe en los 10.000 bytes habituales, para evitar
reintentos de un elemento por llamada. Conservar el
significado de `required_bytes`: la inspección completa con su objeto. Contar la
acción y la advertencia en el suelo de respuesta, y ejecutar el reintento para
comprobar que avanza. Un conflicto devuelve en feedback un reinicio sin `page`,
que recupera el objeto nuevo aunque las páginas anteriores lo reutilizasen. Verificar el rechazo si cambian y medir bytes y llamadas de todo el
recorrido, incluida la primera página; omitir un cuerpo repetido no autoriza
resumir ni eliminar evidencia.

En los verbos temporales, `page` cuenta elementos de entradas y prueba del mismo
paquete; `selection` identifica la selección limitada del núcleo. Ejecutar
`next_actions` con sus argumentos completos: primero reconstruir ese paquete y
después navegar la historia que quede fuera. No convertir `page.next_cursor` en
una referencia de memoria. Una prueba de continuación debe conservar los filtros,
el reloj y la prueba, y comprobar que el cursor rechaza cambios del contenido.
Medir el recorrido completo: cuatro páginas pueden costar más tokens que una
respuesta grande aunque permitan avanzar con un límite menor por respuesta.

Al cambiar `fields`, comprobar qué se proyecta: en los verbos temporales sólo
las entradas; prueba y auditoría dependen de `include`. Conservar identidad,
declarar campos omitidos y ejecutar las acciones de ampliación con el mismo
alcance. Un cambio en contenido oculto también debe invalidar el cursor.
La ampliación es una lectura nueva, no una promesa de snapshot. Medir tanto
navegación selectiva como ampliación de todos los resultados.

La regla de no repetir es sobre la carga en contexto. Un archivo fuente y sus
assets generados no son dos manuales que el agente deba leer. Las reglas comunes
que ya llegan en initialize tampoco necesitan copiarse íntegramente en cada
descripción. Comprobar lo que el host presenta antes de atribuirle al servidor
una repetición añadida fuera del MCP nativo.

Los campos del schema ya declaran tipos, valores admitidos, límites y defaults.
Sus descripciones explican las decisiones que esos campos no expresan: qué
reloj seleccionar, qué oculta un filtro, cómo continuar y qué cuenta como prueba.
Los ejemplos y el detalle consultable pueden desarrollar esas explicaciones.
Al acortar descripciones, comparar también el resto del schema antes y después:
una reducción de texto no debería ocultar la pérdida de un campo o una condición.
Esa comparación es evidencia de la revisión, no un nuevo gate editorial.

Las instrucciones de initialize contienen sólo decisiones comunes entre verbos:
activación, recuperación inicial, elección temporal/semántica, continuidad,
identificadores, prueba y frontera de confianza. El uso específico pertenece
al tool: Ask explica su reintento y UNKNOWN; Forward explica el límite inclusivo
del intervalo; los campos del escritor explican resumen de búsqueda y evidencia.
El detalle y los ejemplos viven en la guía consultable. Algunos hosts anteponen
initialize a cada descripción: medir esa representación evita multiplicar un
manual común. No acoplar esta distribución a tests de frases o de orden del texto.

### Huellas de selección y cambios del transporte

En Wake, `scope.selection` identifica la prueba y las secciones sujetas al reloj
y al intervalo o instante declarados en `proof`. `scope.context` identifica el
contexto del about, cuyo tiempo es `unbounded`; puede describir recuerdos fuera
de esa selección histórica. `scope.dimensions` declara los filtros aplicados a
ambos grupos, con sus valores y predicados. El catálogo conserva las etiquetas
de las entradas que pasan: filtrar por una etiqueta no elimina sus otras etiquetas.
Conservar esta señal dentro del presupuesto y a través del transporte tipado.
Verificar una ventana con prueba, otra sin prueba pero con contexto, y filtros
dimensionales; un about inexistente es un caso distinto. `wake.objective` procede
del solicitante, no es evidencia recuperada. Enseñar estas diferencias en el
verbo y en una lección con fuentes, también visible en ChronoLoom.

Trace y Relate calculan su huella sobre la selección completa antes de cortar
la página. Al añadir un campo a esos resultados, incluirlo en la huella si
cambia contenido, evidencia o alcance observables. El orden de mapas de metadata
no tiene significado: su representación para el hash debe ser determinista.
No ligar el hash sólo a la página que cabe ni transferir toda la selección al
cliente para validarla. El MCP liga la huella a la consulta y cuenta también
las acciones de continuación dentro del presupuesto de respuesta.

Los campos de transporte se editan en `api/proto` y se sincronizan con la copia
de `crates/kmp-proto/proto`. El MCP y el kernel deben compartir ese contrato;
un backend sin huella se rechaza explícitamente. Comprobar continuación, cambio
en una página posterior y acción de reinicio en los transportes afectados.
Esta explicación es mantenimiento informativo; no añade un control de CI.

### Continuaciones de Ask y Wake

`kmp-proto-mapping/src/v1beta1/recall_projection.rs` cuenta la acción antes
que la expansión. `recall_actions.rs` construye la llamada y su presupuesto
suficiente. Al añadir un argumento a Ask/Wake, actualizar su conversión a
argumentos nativos, la identidad del cursor y la prueba que ejecuta la llamada
recibida. Conservar selección temporal, dimensiones y `asked_as`; omitir
`max_entries` ilimitado en vez de enviar un cero rechazado por el MCP.

La respuesta MCP usa `projection.next_action: {tool, arguments}`. El proto
transporta `RecallCall` en `next_call`, con argumentos JSON para conservar
enteros exactamente; el antiguo campo de prosa queda sin emitir y no tiene
adaptador de compatibilidad. Los errores tipados transportan `restart` y el
MCP lo expone como `feedback[].action`. Mantener ambas copias del proto y
comprobar ejecución equivalente en los transportes.

Una acción sin cursor tras `core_text_shortened` restaura el núcleo con una
lectura nueva: descartar la reconstrucción parcial anterior. Las continuaciones
ordinarias añaden sólo expansiones. Probar el recorrido completo contra una
lectura amplia del mismo store, restauración desde 512 bytes, límites y errores
sin analizar el texto del diagnóstico. Medir argumentos más respuestas de todo
el recorrido, incluyendo reinicios. La recuperación propone el tamaño suficiente
más 10.000 bytes para expansión; ofrecer sólo el mínimo puede multiplicar llamadas.
En paridad, aislar XDG_DATA_HOME para que el almacén local no cargue una tabla
léxica personal ausente en el servidor de prueba; comparar la misma configuración
de recuperación en ambos caminos. La comprobación nativa no mide aprendizaje
LLM ni facturación del host. Estas instrucciones son documentación informativa.

### Descubrimiento progresivo y errores

`kmp_guide` sirve el esquema y una ficha por consulta. Identidad y contextos
viven en `agent-users.sqlite3` junto al store embedded; para gRPC son metadatos
locales del cliente, separados por endpoint. No forman parte de la memoria,
la evidencia, los bundles ni la autorización. Una conexión compartida no
identifica al agente: el cliente conserva los ids devueltos.

La clave de registro identifica un agente lógico y hace idempotente su alta.
`context_id` basta para continuar; `agent_id` con una nueva `context_key`
identifica un reinicio de contexto tras compactación. `served` registra entrega,
no aprendizaje ni retención. `fold` cambia lo expandido y conserva esa entrega.
Un cambio de revisión de guía abre una vista nueva de entregas sin borrar el
historial. El cliente debe recuperar las fichas que necesite de nuevo.

Las fichas breves viven en `guide/cards/`; los verbos extendidos en `verbs/`.
El mapper estampa el digest del asset en cada nodo: cambiar contenido exige
regenerar el conjunto. Al añadir un tema, actualizar esquema, schema, ficha,
verbo y relación entre ambos; ejecutar las acciones de ampliación devueltas.
Initialize y las skills entran por el esquema; el Markdown es la alternativa.
No duplicar ambas cargas. Reutilizar el target de este checkout para iterar;
no repetir suites ya aprobadas si no cambia el comportamiento que comprueban.


La entrada generada muestra los grupos de verbos y dos mapas de ejemplos. Los
temas y lecciones completos siguen en KMP. Consultar el verbo antes del primer
uso si no está en contexto y abrir el ejemplo necesario; no cargar todos los
cuerpos para completar el índice. El registro de una consulta anterior no prueba
comprensión ni presencia en el contexto actual.

La ayuda de rechazo se decide por nombre de herramienta, código y ruta del
feedback, nunca analizando su mensaje. `help.guide` y `help.examples` contienen
llamadas completas a nodos existentes. Conservar las acciones directas de
reparación y reinicio, los códigos del backend y la ausencia de escrituras en
un rechazo. La consulta de guía no reintenta ni sincroniza automáticamente. Los
fallos de infraestructura y de lectura de la guía no generan un bucle de ayuda.
UNKNOWN sigue siendo una respuesta semántica válida.

Probar que los enlaces se ejecutan contra los assets generados, incluyendo las
herramientas que comparten verbo. Medir la entrada y las lecturas posteriores:
mover una lista a un nodo no elimina su coste cuando ese nodo se consulta. La
misma ayuda aparece en texto para hosts que no muestran structuredContent;
contar por separado esa representación y el sobre completo.

## 3. Regenerar con el motor correspondiente

Desde la raíz del repo, usar un target separado si hay un binario congelado
para mediciones. No sobrescribirlo ni sincronizar ejemplos de desarrollo en
el store personal.

Al alternar worktrees, preferir un target de desarrollo por checkout. Un target
compartido puede conservar assets de otro checkout aunque el binario arranque.
Si ocurre, limpiar sólo el crate afectado y reconstruir sus dependientes antes
de generar o revisar. Comprobar el resultado visible y registrar el hash del
binario usado; no asumir que una captura representa el código por su ruta.

```bash
export CARGO_TARGET_DIR="$PWD/target/guide-development"
cargo build --locked -p kmp-mcp
cargo run --locked --quiet -p kmp-release -- guide assets write \
  --binary "$CARGO_TARGET_DIR/debug/kmp-mcp"
```

La generación usa el contrato vivo del binario y reconstruye los dos abouts
de guía. Revisar juntos fuente y salidas. Si cambia el relato público:

```bash
cargo run --locked --quiet -p kmp-release -- readme sync
```

Si cambia intencionalmente lo que MCP anuncia o devuelve, regenerar sus
fixtures y revisar el diff completo. Este paso no corresponde a una mera
reorganización editorial:

```bash
KMP_BLESS_TOOL_SURFACE=1 cargo test --locked -p kmp-mcp --test tool_surface_parity
```

## 4. Revisar el uso y comprobar el comportamiento

Para un cambio de guía o routing, ejecutar:

```bash
cargo test --locked -p kmp-release --test guide_editorial_contract \
  --test plugin_package_contract
cargo test --locked -p kmp-mcp --test guide_sync
cargo test --locked -p kmp-adapter-embedded --test guide_bundle
python3 scripts/ci/kmp-capability-contract.py
```

La redacción, extensión y organización de la guía se revisan informativamente.
No crear validadores de frases, límites arbitrarios de tamaño o simulaciones
que sólo comparen trazas ya escritas en fixtures. Tampoco hace falta un dominio
ni capas de aplicación dedicadas a imponer ese procedimiento. Las pruebas
nativas comprueban la operación que ejecuta KMP; el documento explica su uso.

Ejecutar además las pruebas del comportamiento afectado. Para Rust, usar fmt
y clippy conforme a [Testing](testing.md). Elegir los checks según el cambio;
no actualizar expectativas de evidencia, relojes o selección sin justificarlo.

Reproducir una lección afectada en un store aislado:

```bash
guide_review_dir="artifacts/guide-review-$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$guide_review_dir"
python3 scripts/guide_examples/replay.py \
  --binary "$CARGO_TARGET_DIR/debug/kmp-mcp" \
  --lesson alias-ownership --guide-mode markdown \
  --trace "$guide_review_dir/native.jsonl" \
  --result "$guide_review_dir/result.json"
```

Usar el caso nuevo cuando exista. Comprobar lectura de la entrada una vez,
acceso al verbo necesario, continuación de páginas y reuso de guía mientras
el mismo asset permanezca en contexto. No reutilizar respuestas de memoria
cambiante como si fueran documentación estática. Tras compactación, cambio
de store o de asset, revalidar lo que sigue disponible.

Para cambios en representación, navegación o guía humana, añadir `--hold-view`
al replay y revisar el contexto en ChronoLoom: about, reloj, intervalo,
memorias, etiquetas, dirección de relaciones y fuentes. Guardar la observación
y cerrar el visor temporal. No conservar URLs de capacidad con sus credenciales.

## 5. Medir lo que realmente ve el agente

Registrar por separado estas representaciones:

- respuesta MCP nativa de initialize;
- respuesta MCP nativa de tools/list, con descripciones y schemas;
- catálogo o metadatos que expone el host;
- skill y entrada inicial;
- consultas de guía y resultados del recorrido de trabajo.

Comparar también la firma callable que ve el agente con el schema nativo.
Un objeto válido en tools/list puede aparecer como `unknown` si el host prioriza
una composición condicional y omite sus propiedades hermanas. Revisar ese caso
con la representación observada, mantener las restricciones del contrato y
registrar la comprobación después de cargar el cambio. Una proyección local no
prueba que una sesión abierta haya renovado sus herramientas.

Contar bytes y tokens con el nombre, versión y configuración del tokenizer.
Conservar texto o trazas y hashes. Comparar primer uso, primera consulta a un
verbo, repetición de ese verbo y recorrido completo con cobertura equivalente.
Si se elimina detalle del contexto inicial, contar también la lectura posterior
de ese detalle. No sumar dos representaciones alternativas como si ambas se
inyectaran, ni presentar estos conteos como facturación o ahorro monetario.

Para errores de escritura, asignar código y ruta donde se valida el campo.
No deducirlos del texto del error. Conservar la categoría de un fallo del backend;
una instrucción de reparación no debe convertirlo en un error del llamante.
Si se propone una lectura, incluir nombre y argumentos completos y probar que
puede ejecutarse. Si falta evidencia, señalar el campo sin fabricar un payload.
El vocabulario de tipos del escritor vive en
`contract/writer_memory_kinds.rs`: schema, validación y `allowed_values` lo
consumen directamente. Al ampliarlo, enseñar cuándo corresponde el tipo y
verificar rechazo, elección basada en fuente y commit; nunca añadir un reemplazo
automático deducido del nombre inválido. Revisar también los errores de esquema anteriores al planner y los índices del
paquete. Las pruebas deben recorrer rechazo, lectura/corrección y commit, con
verificación de que el paquete rechazado no produjo escrituras parciales.

Cuando el cambio dependa de cómo aprende o decide un LLM, probarlo por separado
con fuentes nuevas y preguntas ocultas al escritor, dentro del presupuesto y
estado de pausa autorizados. La validación nativa no reanuda por sí sola una
medición con modelos.

## 6. Dejar un cierre que permita continuar

La entrega debe identificar commit, fuentes modificadas, assets regenerados,
pruebas ejecutadas, coste antes/después y límites. Indicar si sólo está en una
rama, si se ha mergeado o si ya está instalado. Una sesión abierta puede seguir
exponiendo metadatos anteriores: no confundir el código entregado con la
superficie que esa sesión utiliza.

Conservar los defectos y acciones pendientes en su issue. Un incremento de
Markdown no completa una revisión de toda la superficie. Los bugs siguen su
PR y checks; los cambios de capacidad siguen el flujo de iteración acordado.
La publicación e instalación se rigen por [Releasing](releasing.md).

Actualizar este procedimiento cuando cambie un propietario, un asset, una
regla de carga o un comando. `AGENTS.md`, el índice de desarrollo y el README
de la guía deben seguir apuntando aquí, para que una persona y un agente
encuentren el mismo contrato de mantenimiento.

Writer receipts use the accepted command event and idempotency index. Keep the
compact acknowledgment, native/gRPC ingest mapping and Inspect audit detail in
step. Verify actual returned actions across restart, subsequent memory changes
and export/import; previews and refusals must create neither a receipt nor memory.
Coverage describes submitted declarations, never completeness against sources the
writer did not submit. Do not add graph memories or another CI rule for receipts.

### Contexto y recomendaciones del agente

Los argumentos comunes context_id/purpose se agregan en el registro de tools.
CallGuidance los resuelve y elimina antes de compilar, calcular idempotencia o
llamar al backend. No son una autorización. El escritor y Relabel pueden tomar
actor del perfil; un actor explícito prevalece. AgentDirectory guarda uso y
entregas fuera del grafo. Una entrega no acredita aprendizaje.

GuidanceRecommendation elige por campos tipados del resultado, conserva las
acciones nativas y declara cuándo inicia otra selección. Un verbo nuevo necesita
su ruta de ayuda, criterios de recomendación sólo si tiene una señal real, y
comprobación de que la acción devuelta se ejecuta sin perder reloj o alcance.
No crear reglas por palabras de mensajes de error ni puntuaciones de confianza
sin fundamento. La tabla consultable está en verbs/guide.md.

La ayuda de trabajo se serializa una vez en un bloque de texto kmp_guidance:
structuredContent conserva su contrato y presupuesto. Medir ambos costes y
comprobar que el host entrega el bloque adicional al modelo. Una aplicación que
sólo conserva structuredContent oculta la ayuda. El fallo de metadatos después
de una escritura aceptada debe conservar la aceptación y el recibo; el contador
de uso nunca certifica una nueva escritura frente a un replay.
