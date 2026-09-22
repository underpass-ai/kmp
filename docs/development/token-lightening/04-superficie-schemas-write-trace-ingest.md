# 04 — Adelgazar schemas de write/trace/ingest

## Problema

Los schemas JSON de las tres herramientas de escritura/traza concentran el 35%
de la superficie estática. El enum de 31 relaciones de `connect_to`/`relations`
está duplicado completo en DOS campos (`memories[].connect_to[].rel` y
`relations[].rel`), y cada opción lleva una descripción de ~250 tokens con el
vocabulario entero repetido dentro de la descripción del enum.

## Evidencia (reporte live del harness)

```
kmp_write_memory  7.739 tok  (input schema 5.019)
kmp_trace         6.118 tok  (input schema 3.970)
kmp_ingest        4.173 tok  (input schema 3.078)
subtotal:        18.030 tok = 35% de la superficie live (50.976)
```

Sólo el campo `rel` (enum + descripción con el vocabulario completo) aparece
dos veces en write_memory y pesa ~1.500 tok por copia.

## Propuesta

1. **Descripciones del enum `rel` → una sola referencia**: la descripción
   completa del vocabulario ya vive en la guía (`guide:kmp-agent:card:write`).
   En el schema, `"rel": {"enum": [...], "description": "See guide card write."}`.
   Ahorro: ~1.300 tok × 2 campos × 2 herramientas ≈ 5.000 tok.
2. **Unificar `connect_to` y `relations`**: `relations` acepta local ids como
   `from`, y `connect_to` desaparece. Un solo lugar con el vocabulario.
3. **`kmp_ingest` no es para agentes** ("Reach for kmp_ingest only when
   producing the exact graph yourself"): evaluar sacarlo de tools/list por
   defecto y habilitarlo por flag del host. −4.173 tok.

## Impacto esperado

- Superficie estática: 50.976 → ~41.000 tok (−20%).
- Coste por sesión host: $0,52 → $0,41 en Astra estándar. En flotas con miles
  de sesiones, es la mejora de mayor ROI absoluto.

## Riesgos

- Los agentes pierden el vocabulario de relaciones en-linea; Mitigación: la
  card `write` del guide ya lo explica completo (medido: 1.010 tok una vez,
  bajo demanda).
- `kmp_ingest` oculto rompe a quien lo usa legítimamente; hacer opt-in, no
  eliminar.

## Esfuerzo

Bajo para (1) y (3) — edición de descripciones en el contract builder.
Medio para (2) — requiere deprecación de `connect_to`.
