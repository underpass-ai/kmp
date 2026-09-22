# 10 — jev-enrich + jev-route: auditoría Jev de relaciones (DISEÑO VALIDADO)

## Idea

Post-escritura, un comando audita las relaciones de un about con Jev
(System One, $0.042/Mtok) y produce un informe de mejora: qué aristas son
anémicas, cuáles absurdas, cuáles mantener. El LLM, a partir del informe,
reescribe o introduce las relaciones que faltan. Loop: write → audit → enrich.

## Calibración (2026-09-22, 7 intentos — el camino hasta el diseño correcto)

| v | Diseño | Resultado |
|---|---|---|
| v1 | noul por arista, why+evidence en state | FALLO: real 0.33 vs corrupta 0.37, sin separación |
| v2 | noul con textos completos | Anclaje de framing, no verificación (0.34→0.68 sin discriminación) |
| v3 | 10 textos en un state, criteria null | FALLO: todo "other" (claves no ligadas a textos) |
| v4-v5 | choice 1 texto/request, criteria descriptivas | 4/4 correctas conf 1.0 — clasificación SÍ funciona |
| v5-score | score multi-nivel sobre relaciones reales | Colapso: todo ~1.5 (contaminación multi-pregunta) |
| v7 | **noul 1 relación por request, aislada** | **Separación perfecta: real 0.57, medio 0.37, absurdo 0.03** |

**Lección central**: Jev juzga bien en aislamiento y mal en batch. El fan-out
correcto es N requests paralelos de 1 relación (~350 tok c/u), no 1 request con
N preguntas. El absurdo deliberado ("guide depends on the coffee machine")
cae a noul 0.03 con confidence alta — Jev sí detecta incongruencia cuando no
hay contaminación de contexto.

**Segunda lección**: los ref-slugs crudos envenenan el juicio (v3: todo ~0.05).
El exportador debe renderizar las relaciones en lenguaje humano antes de
consultar a Jev.

## Pipeline validado

```
1. Export      kmp_wake (o trace) del about -> relaciones con why/evidence
2. Render      traducir refs a texto legible (claim real, no slug)
3. Audit       1 request Jev noul por relación (aislada, paralela):
               "¿la evidencia citada soporta concretamente el claim?"
4. Report      noul >= 0.6  -> mantener
               0.4 - 0.6    -> moderada: mejorar evidence
               noul < 0.4   -> anémica/absurda: reescribir o eliminar
5. Enrich      el LLM (yo) recibe el informe y escribe las correcciones
               (kmp_write_memory con corrects/supersedes)
```

## Coste medido

- 1 relación aislada: ~350 tok = **$0.0000147**.
- El probe (13 relaciones): $0.000044, <1 s.
- About real (1.270 relaciones): ~$0.019, ~1 min (1.200 req/min de límite).
- ~20× más barato que LLM frontier, salida estructurada sin alucinación.

## Límites honestos

- No detecta evidencia de la misma superficie pero equivocada (v1); el
  cross-check por categoría (v5 de clasificación) complementa esto.
- El rendering importa: rels con slugs crudos puntúan mal aunque sean ricas;
  el comando debe renderizar siempre.
- Jev prioriza inglés; renderizar en inglés si confidence baja.

## Scripts

- `jev-route.sh` — reranking de rutas de grafo para responder un ask/wake por
  el camino más barato (trace discovery -> Jev elige -> ejecutar esa ruta).
  Medido: overhead $0.00015 por pregunta; ahorro $0.16 vs scan completo.
- `jev-audit` (este diseño) — informe de mejora de todas las relaciones.
