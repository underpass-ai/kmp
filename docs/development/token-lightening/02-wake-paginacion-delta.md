# 02 — Paginación delta en continuaciones de `kmp_wake`

## Problema

Las continuaciones de wake (`continuation: read_*`) reenvían en cada página
bloques idénticos a la página anterior:

- envelope `kmp_guidance` completo (recommendation, usage, guide_revision)
- `proof.superseded` (mismos items en todas las páginas)
- `scope`, `summary`, `resume_cursor`, `truncation`
- `wake.causal_spine`, `wake.current_state`, `wake.next_actions` ya servidos

## Evidencia (6 páginas del probe, 9 nodos)

```
Bloques repetidos por página:          624 tok
× 5 continuaciones:                  3.120 tok = 21% del wake total (14.430 tok)
Extrapolado a project:kmp (~40 pág): ~24.000 tok solo en repetición
Coste Astra de la repetición:        $0,03 (probe) / $0,24 (about real)
```

## Propuesta

Modo `delta` para continuaciones: la respuesta de una continuación contiene
solo `page` + los bloques que hayan cambiado. El contrato:

```json
{"delta": {"page": {...}, "new_evidence": [...], "changed": ["labels"]}}
```

- Página 1: packet completo (igual que hoy).
- Páginas 2+: solo expansión nueva. Si un bloque cambió (p. ej. apareció un
  conflicto), se envía completo y se lista en `changed`.
- El host reconstruye el packet completo concatenando.

Alternativa menor (si delta es demasiado invasivo): marcar bloques ya servidos
con `"unchanged": true` y omitir su contenido.

## Impacto esperado

- −20% del wake en abouts con muchas páginas.
- En abouts enormes (40+ páginas) el ahorro absoluto es mayor: ~$0,24 → ~$0,05
  por lectura completa en Astra.

## Riesgos

- El host debe mantener estado entre llamadas (ya lo hace: el cursor).
- Debugging algo más difícil sin el packet íntegro; mitigar con
  `budget.replay_full: true` para reenviar todo bajo demanda.

## Esfuerzo

Medio-bajo. El mecanismo de continuaciones `read_*` ya preserva la selección;
es serializar menos, no leer diferente.
