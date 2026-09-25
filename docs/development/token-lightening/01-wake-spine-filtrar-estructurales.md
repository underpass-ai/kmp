# 01 — Filtrar aristas estructurales de `wake.causal_spine`

## Problema

`wake.causal_spine` materializa todas las relaciones del about, incluidas las
estructurales triviales que el propio kernel genera: `anchor -> label`,
`label -> entry`, `evidence -> entry`, `anchor -> entry`. En un about recién
creado con UN registro y 3 labels, la spine ya contenía 8 items de los cuales
7 eran estructura. Con 9 nodos: 8 items, 7 estructurales.

Medido (about de 1 nodo): el wake consumió 7.525 de 10.000 bytes — 75% del
techo en estructura de contención, no en contenido semántico.

## Evidencia

```
about con 1 nodo:  causal_spine = 8 items, 7 estructurales (87%)
about con 9 nodos: causal_spine = 8 items en page 1, todos estructurales salvo 1 depends_on
excluded_by_detail: 8 (1 nodo) -> 71 (9 nodos): la estructura crece linealmente
```

## Propuesta

En la proyección `wake`:

1. Excluir por defecto las relaciones de clase `structural` de
   `wake.causal_spine` (las de vocabulario `contains`, `member_of`,
   `scoped_to` y las aristas de contención generadas por el kernel).
2. Ofrecer `budget.include_structure: true` para quien las necesite.
3. Las aristas `supports` generadas automáticamente (evidencia → entry)
   son las más numerosas: agruparlas como un contador
   (`"evidence_count": 9`) en vez de 9 items.

## Impacto esperado

- Wake de about pequeño: de ~1.900 a ~800 tokens (−58%).
- Wake de about mediano (100 nodos): baja de imposible a ~1 página más el
  contenido semántico real.
- Reduce también `excluded_by_detail`, que hoy es abrumadoramente estructura.

## Riesgos

- Un agente que hoy usa la spine para descubrir memberships perdería esa vía;
  compensar documentando que labels siguen en `labels` (ya están ahí).
- Cambio de contract de proyección: bump de `kmp.recall.projection.v1` a v2
  o flag opt-in para compatibilidad.

## Esfuerzo

Medio. Toca el selector de la spine en el projection builder; sin cambios de
store.
