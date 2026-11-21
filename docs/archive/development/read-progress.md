# Hacer visible una lectura pendiente

En Cala, Sol recibió dos entradas temporales y dejó siete relaciones por leer.
La respuesta contenía `page.has_more:true` y una continuación; el mensaje principal
solo decía «Returned 2 temporal entries». `selection.has_more:false` describía
la selección de entradas, no la proyección de su prueba. Terra también terminó
tras un Wake parcial. F-134 y [#544](https://github.com/underpass-ai/kmp/issues/544)
conservan esas reproducciones y entrevistas.

## Decisión

Una lectura exitosa pero parcial empieza su primer texto MCP con
`READ_INCOMPLETE`. El aviso explica que faltan partes del paquete seleccionado y
que se debe seguir la acción devuelta o conservar explícitamente el resultado
parcial. Después aparece el resumen original. Se sirve sin exigir identidad,
contexto, consumo de guía ni persistencia adicional.

La señal deriva de los campos nativos, sin interpretar la prosa:

- `page.has_more:true`;
- `projection.page.has_more:true`;
- `projection.core_text_shortened:true`, aunque ya no queden elementos de expansión.

Las ayudas opcionales y este aviso usan la misma detección. La señal particular
de vecindad del escritor conserva su ámbito; no se confunde con una lectura.
Una selección temporal que ofrece más historia pero ya entregó toda su prueba
no recibe el aviso. Tampoco se usa el mero hecho de tener `next_actions`: una
acción puede iniciar otra selección. UNKNOWN completo conserva su significado.
El fin de página nunca certifica suficiencia semántica ni búsqueda exhaustiva.

## Límite de la modificación

Se cambia el texto del sobre MCP, no el objeto canónico. `structuredContent`,
sus bytes, fuentes, relojes, omisiones, índices, cursores y presupuestos permanecen
iguales. El texto principal añade un aviso breve, sin duplicar pruebas, acciones
completas ni lecciones. Los clientes que usan exclusivamente structuredContent
siguen comprobando sus campos nativos. No se añade otro estado persistido, otro
campo redundante de finalización o una barrera que controle la respuesta del LLM.

Esto pretende que la señal existente llegue antes al lector. No demuestra que
un agente la siga; se comprobará en una tarea nueva después de la entrega. Los
intentos Cala cerrados permanecen parciales y no se reparan retrospectivamente.

## Aceptación

Congelar y reproducir Wake y Goto reales en copias de la escritura Cala aprobada,
sin los contextos del ensayo, y completar únicamente sus paquetes. Añadir Inspect,
Trace, UNKNOWN completo y selección terminada con más historia. Comparar todas
las páginas estructuradas exactamente antes/después y medir el sobre completo.
Probar además núcleo abreviado sin más páginas, error, llamada sin registro y
coherencia del indicador opcional. Conservar el texto del resumen y el contenido
canónico byte por byte.

Actualizar entrada breve, tarjeta temporal, guía ampliada, ejemplo de presupuesto
y guía humana desde sus fuentes. Revisar las superficies nativas reducidas y
los fixtures de respuesta que cambien; no añadir checks editoriales de CI. La
compresión y los demás requisitos del objetivo siguen su seguimiento separado.

## Resultado de la comprobación

V2 conserva exactamente 22 páginas y añade 16 avisos; V3 verifica otra página
completa con más historia disponible y sin aviso. Son 54 RPC entre los dos
controles emparejados, sin errores ni modelos. V1 se conserva con 10 RPC y un
rechazo correcto porque el investigador pidió `page.entries` en Inspect; V2
retira ese argumento. El control V2 llamado `goto-more-history` no produjo esa
condición y no se usa para probarla; V3 añade el `limit.entries` documentado sin
cambiar código ni fuentes.

El aviso añade 26 tokens o200k por página parcial: 416 tokens y 2176 bytes en V2,
sin cambiar structuredContent. Catálogo e inicialización nativos iguales.
Entrada del agente: 359 → 389 tokens; tarjeta temporal: 328 → 433. No se afirma
ahorro ni comprensión nueva. Pasan 613 tests MCP; uno manual previo permanece
omitido. Clippy y generación de guía pasan. El control adicional shared_passages
comprueba las nueve lecturas, expansión exacta y coherencia del primer texto con
la ayuda opcional. No se añaden barreras editoriales de CI.
