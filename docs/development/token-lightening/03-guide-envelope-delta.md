# 03 — Envelope delta en `kmp_guide`

## Problema

Cada llamada a `kmp_guide` (incluida cada expansión de topic) devuelve el
envelope completo:

- `agent` (id, name)
- `scheme` completo (9 items con purpose+topic)
- `served` acumulado (lista creciente de topics ya vistos)
- `summary`, `used` (stats de uso acumuladas), `next_actions`
- `guide_revision`, `context_id`, `durable`

## Evidencia

```
10 llamadas (registro + 9 topics):
  contenido útil (cards):            ~2.400 tok
  envelopes repetidos:               ~7.000 tok  = 73% del coste de la guía
  total:                             ~9.500-10.600 tok → $0,10 en Astra
```

El scheme de 9 items (que el agente ya recibió en el registro) viaja idéntico
10 veces. `served` crece por llamada. `used` agrega contadores que nadie
consume a mitad de tarea.

## Propuesta

1. Tras el registro, las respuestas de expansión devuelven solo:
   `{card, next_actions}` + `guide_revision` (para detectar cambios).
2. `scheme` completo solo en el registro o bajo petición
   (`{"scheme": true}`).
3. `served` y `used` solo bajo petición (útiles para audits, no para leer).

## Impacto esperado

- Leer la guía completa: de ~10.600 a ~3.500 tokens (−67%).
- Coste Astra de la guía: $0,10 → $0,035.

## Riesgos

- Bajo: el agente que necesite el scheme lo repide; ningún flujo existente
  depende del envelope salvo el propio guide.

## Esfuerzo

Bajo. Cambio de forma de respuesta de una herramienta, sin tocar el store.
