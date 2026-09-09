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
un tool, revisar también inventario, registro, mapping de guía, fixtures y
el descubrimiento real en el host. Evitar repetir un recuento fijo en varios tests.

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

Al cambiar la representación de una continuación, comprobar la reconstrucción
completa desde sus páginas contra una lectura sin paginar. En Inspect,
`page.repeat_object=false` sólo sirve con un cursor y el objeto inicial
conservado; `object_reused=true` y `object.ref` identifican lo reutilizado.
Texto, metadatos, fuente y pruebas siguen vinculados al cursor, aunque no se
repitan. Verificar el rechazo si cambian y medir bytes y llamadas de todo el
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
Revisar también los errores de esquema anteriores al planner y los índices del
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
