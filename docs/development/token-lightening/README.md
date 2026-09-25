# Mejoras de economía de tokens para KMP

Directorio de propuestas de mejora para reducir el consumo de tokens de KMP
como superficie de un LLM host. Cada mejora tiene su propio `.md` con el
problema, la evidencia medida (medición del 2026-09-22, counter tiktoken
o200k_base, precios Astra $10/$50 por 1M), la propuesta y los riesgos.

## Contexto de medición

- Sesión real: about nuevo `experiment:about-cost-probe` (9 nodos), wake de
  `project:kmp` (1.270 entries), ciclo completo de escritura con validación
  estricta, lectura completa de la guía (10 topics).
- Harness: `~/Documents/ai/tools/kmp-token-mcp` (tiktoken, o200k_base).
- Totales de la sesión: ~71k in / 51 out → **$0,71 en Astra estándar**
  ($0,36 batch, $0,57 con prompt caching).

## Índice por ahorro estimado

| Mejora | Ahorro estimado | Archivo |
|---|---|---|
| Filtrar aristas estructurales de `wake.causal_spine` | ~60% del wake de abouts pequeños | `01-wake-spine-filtrar-estructurales.md` |
| Paginación delta en continuaciones (`kmp_wake`) | ~20% del wake en abouts grandes | `02-wake-paginacion-delta.md` |
| Envelopes delta en `kmp_guide` | ~70% del coste de leer la guía | `03-guide-envelope-delta.md` |
| Adelgazar schemas de write/trace/ingest | ~30% de la superficie estática | `04-superficie-schemas-write-trace-ingest.md` |
| Consolidar verbos temporales gemelos | ~6.500 tok de superficie estática | `05-consolidar-verbos-temporales.md` |
| Receipt de write más compacto | ~40% del coste por write | `06-write-receipt-compacto.md` |
| Detección proactiva de guía desincronizada | evita errores de agente (indirecto) | `07-guide-sync-deteccion-proactiva.md` |
| Navegar con `fields` mínimos + cuerpos lazy | −60-75% en lecturas exploratorias | `08-fields-minimos-verbos-temporales.md` |
| Relaciones ricas se pagan a sí mismas | trace 6.8× más barato que wake | `09-relaciones-ricas-se-pagan.md` |
| **jev-enrich**: auditoría Jev de relaciones | calidad de grafo por ~1% del coste de escritura | `10-jev-enrich.md` |

## Scripts funcionales

- `jev-route.sh <about> <from-ref> <question> [to-ref]` — descubre rutas de
  grafo con `kmp_trace` (proof:false), Jev elige la más barata que responde,
  la ejecuta con proof. Overhead ~$0.00015/pregunta.
- `jev-enrich.sh <about>` — audita TODAS las relaciones del about con Jev
  (1 request noul aislado por relación, paralelo) y produce el informe:
  mantener / mejorar / reescribir. 13 relaciones por $0.00023.

## Resultado del loop completo (verificado en `experiment:about-cost-probe`)

```
audit 1:  1 mantener / 3 mejorar / 9 reescribir   ($0.00023)
corrección LLM: 4 verified_by re-emitidas con evidencia completa (números)
audit 2:  2 mantener / 4 mejorar / 7 reescribir
  D (fase-1): 0.26 -> 0.85   G (fase-4): 0.38 -> 0.55   E (fase-2): 0.48 -> 0.57
```

Las que siguen bajas tras la corrección son de dos tipos: aristas
estructurales del kernel (ruido esperado — ver mejora 01) y duplicados
del edge antiguo que la re-escritura debe supersedar.

## Principio rector

KMP consume 99,9% input tokens en el host (el output es marginal). Las tres
palancas, en orden de impacto: (1) superficie estática que se paga en cada
conversación, (2) proyecciones que re-expanden estructura en cada lectura,
(3) bloques repetidos en paginación full-packet. El output/carro de Astra es
5× el input, pero nada de esto toca output: todo el ahorro es input.
