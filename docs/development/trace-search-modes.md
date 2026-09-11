# Elegir el modo de Trace y reparar una mezcla en un paso

## Problema observado

Tras PR717, dos lectores nativos de Cala mezclaron `search.seek` con
`paths_per_target`; uno añadió también `select`. El catálogo declara todos los
campos en una lista, y la tarjeta reducida explica ambos usos consecutivamente.
El servidor rechaza solo el primer campo incompatible: el segundo lector
necesitó dos reparaciones. El solver sí conservó sus candidatos.

Ese lector también exigió igualdad entre una copia verificada y una publicación
autorizada. Era una condición añadida a la pregunta: el ejemplo de misma acción
se aplicó a dos acciones distintas. Ambos lectores omitieron el corte observado
en Trace. La evidencia original, entrevistas y límites están en el proyecto de
evaluación, `reports/CALA-READERS-20260911.md`, hallazgo F-134; seguimiento en
[#538](https://github.com/underpass-ai/kmp/issues/538) y
[#544](https://github.com/underpass-ai/kmp/issues/544).

## Decisión

Mantener un verbo y los argumentos existentes. Anunciar dos modos excluyentes:

| Intención | Entrada | Controles propios |
| --- | --- | --- |
| Conectar destinos ya conocidos | `from` y `to` | `follow` o `direction`/`relations`, dimensiones, `paths_per_target`, `select` |
| Descubrir pruebas sin destinos conocidos | `from` y `search.seek` | Papeles con relación/sentido/via/after/labels, `same_labels`, `same_ref` |

Comparten `max_nodes`, `max_edges`, `max_depth` y `max_states`. Reloj y paginación
pertenecen a la llamada, no son un tercer modo. El modo se deduce de la presencia
de `seek`: no se añade un selector redundante, otro lenguaje ni otra herramienta.

El esquema declara `oneOf` con nombre y lista de campos permitidos por modo.
Las definiciones de cada campo se conservan una sola vez en `properties`, con
su modo explícito. Así se evita duplicar subesquemas grandes y se conserva la
validación recursiva actual de argumentos desconocidos. No se cambia el validador
genérico ni se introducen referencias de esquema que el host deba resolver.
La exclusión en el esquema ayuda a los clientes que lo validan; el parser nativo
también la exige sin depender de que el agente o el host validen JSON Schema.

El parser comprueba primero los campos de modos mezclados y enumera todos en
orden estable, incluido `to`. No intenta interpretar un `follow` incompatible
antes de explicar la mezcla. El agente retira esos campos en una sola reparación.
Esto no promete enumerar todos los errores de forma, rango o semántica del request.
Un rechazo no ejecuta búsqueda, modifica datos ni sustituye argumentos.

## Enseñanza mínima

La tarjeta empieza por elegir uno de los modos y ofrece una llamada completa con
corte observado. La guía ampliada y el ejemplo distinguen estas preguntas:

- «¿Qué verifica la copia y qué autoriza la publicación?» admite acciones
  distintas. No añadir igualdad de anclas; revisar las fuentes que las conectan.
- «¿Esta misma acción está verificada y autorizada?» requiere igualar sus anclas.
  Un evento compartido o la misma persona no identifican una acción.

Una incompatibilidad exige revisar la formulación y las fuentes. No quitar
igualdades automáticamente para obtener un grupo. `review_required` sigue siendo
revisión pendiente, y `as_of` sigue siendo un corte, no igualdad de timestamps.
Una pregunta exclusivamente histórica se resuelve con navegación temporal.

## Aceptación y control

1. Congelar las llamadas fallidas reales, las llamadas válidas y un control de
   destinos, antes de editar el código. Reproducir contra PR717 y el candidato
   en copias de la misma memoria. Conservar todos los intentos y fuentes.
2. La mezcla original de Luna enumera `select` y `paths_per_target` en un solo
   error. Retirar todos los campos indicados produce la misma petición válida;
   la de Sol enumera su único campo. Una mezcla con `to` lo enumera también.
3. El esquema acepta ambos modos válidos y rechaza sus mezclas, así como las
   uniones sin `seek`. El parser conserva el rechazo de campos desconocidos,
   también dentro de filtros y opciones de destino.
4. Las llamadas válidas conservan exactamente selección, grupos, candidatos,
   relaciones, pruebas, relojes y continuaciones. La incompatibilidad real de
   acciones sigue devolviendo cero grupos; el caso de acciones distintas sigue
   conservando ambos candidatos. El motor no infiere condiciones de la pregunta.
5. Regenerar guía del agente y humana, catálogo normal y con Apps. Medir catálogo,
   tarjeta y errores completos; no confundir bytes con tokens facturados ni
   prometer menos errores de agentes sin otro ensayo independiente.

No cambia dominio, protobuf, almacenamiento, puntuación ni límites de búsqueda.
La señal de selección terminada con prueba paginada pendiente se aborda en otro
incremento bajo #544. La compresión por frases del lector viene después.
Evolución pequeña sobre integración, comprobaciones locales y squash; sin nueva
barrera editorial de CI, release o instalación.

## Resultado del control local

Once llamadas congeladas sobre copias de la misma escritura:13 RPC por binario,
26 en total, sin modelos. Las cinco consultas válidas conservan exactamente su
structuredContent, incluidos grupos, candidatos, relaciones y relojes. Seis
rechazos por lado permanecen; las cuatro mezclas enumeran todos sus campos
incompatibles en el candidato. El esquema JSON distingue los modos válidos de
las mezclas y sigue rechazando uniones sin seek y campos desconocidos anidados.
No se ha repetido a los lectores ni atribuido comprensión a este replay.

Medición o200k_base sobre JSON compacto/texto literal: definición Trace
4467→4712 tokens; tarjeta audit726→661. El catálogo completo añade245 tokens y
solo cambia Trace, tanto normal como con Apps. El primer rechazo de Luna crece
317→380 tokens al reunir sus campos. No es ahorro de una interacción completa
ni tokens facturados. Fuentes operativas en kmp-eval/artifacts/trace-search-modes-20260911-v1.
