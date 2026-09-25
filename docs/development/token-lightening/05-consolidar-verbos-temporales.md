# 05 — Consolidar verbos temporales gemelos

## Problema

`kmp_goto`, `kmp_near`, `kmp_rewind` y `kmp_forward` son cuatro herramientas
separadas con schemas casi idénticos (input ~1.550 tok, output 1.776 tok cada
una). Todas navegan el mismo eje temporal; se diferencian solo en la forma del
argumento (posición / vecindad / retroceso / avance).

## Evidencia

```
kmp_rewind    3.384 tok
kmp_forward   3.383 tok
kmp_goto      3.374 tok
kmp_near      3.368 tok
subtotal:    13.509 tok = 26% de la superficie live
```

## Propuesta

Una sola herramienta `kmp_time` con el modo en el argumento:

```json
{"about": "...", "mode": "goto|near|rewind|forward", "at": "...", "interval": {...}}
```

El schema de input de los cuatro es esencialmente el mismo (las descripciones
de `as_of`, `interval`, `axis`, `budget`, `dimensions` son idénticas), así que
el schema unificado pesa ~input de uno + 200 tok de enum de modos.

## Impacto esperado

- Superficie: −13.509 + 3.600 ≈ **−9.900 tok** (−19% adicional sobre la
  superficie post-04).
- Coste de carga por sesión: −$0,10 en Astra.

## Riesgos

- Rompe scripts/skills existentes que llaman `kmp_rewind` etc. Deprecar con
  alias de un release (los alias heredan el schema del unificado, sin coste
  extra si el host deduplica… ojo: los alias NO ahorran en tools/list, así que
  el ahorro solo llega cuando el host retire los nombres viejos).
- La guía referencia los cuatro verbos por nombre en cards y ejemplos; tocar
  `plugins/kmp/guide/editorial.json` en el mismo cambio (procedimiento de
  surface maintenance del AGENTS.md del repo).

## Esfuerzo

Medio-alto. Contrato nuevo, alias de compatibilidad, regeneración de guías,
actualización de skills externas.
