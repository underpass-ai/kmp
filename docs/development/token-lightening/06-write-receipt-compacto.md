# 06 — Receipt de write más compacto

## Problema

La respuesta de `kmp_write_memory` incluye bloques que duplican información o
que el agente no consume en el flujo normal:

- `clocks`: desglose de distinct_values/single_value por cada reloj para cada
  tipo de entidad — ~400 tok con 8 registros.
- `coverage`: repite los conteos ya evidentes del summary.
- `labels.written` repite `labels.created` cuando son idénticos.
- `receipt.action.arguments`: re-envía los argumentos completos de la llamada
  kmp_inspect de seguimiento (~150 tok).
- `viewer.tell_the_user` + `viewer.url` en cada write (~80 tok).

## Evidencia

```
Receipt del commit (8 entries, 1 relation, 9 evidence): ~700 tok
Payload equivalente en info mínima (status + refs + warnings): ~250 tok
Escribir el plan: 3 llamadas (v1 rechazado, v2 review, resume)
  -> el ciclo completo de escritura costó ~2.900 tok de protocolo
     para 1.124 tok de contenido real
```

## Propuesta

1. `clocks`: solo cuando el caller pasó `observed_at` explícito o hubo
   normalización; por defecto, omitir.
2. `labels`: un solo array con flag `"created": true` por item.
3. `receipt.action.arguments`: reemplazar por `receipt.ref` (el agente ya
   sabe llamar a kmp_inspect; el ref basta).
4. `viewer`: solo en el primer write de la sesión.
5. Añadir `options.receipt: "full"|"compact"` (default compact) para el caso
   de debugging.

## Impacto esperado

- Write de 8 nodos: ~700 → ~250 tok de receipt (−64%).
- Ciclo de escritura completo (con validación+review): ~$0,042 → ~$0,032.
- En pipelines agénticos que escriben mucho (el caso KMP+JEV: Codex escribiendo
  decisiones), es la segunda palanca tras la superficie estática.

## Riesgos

- Bajo. Los campos completos siguen disponibles con `receipt: "full"`.
- El viewer URL es valioso para el humano; mantenerlo pero solo la primera vez.

## Esfuerzo

Bajo. Cambio de serialización de respuesta; el store no se toca.
