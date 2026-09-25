# 08 — Navegar primero con `fields` mínimos y cuerpos lazy en los verbos temporales

## Problema (dos caras)

1. **Documentación**: los cuatro verbos temporales (`kmp_goto`, `kmp_near`,
   `kmp_rewind`, `kmp_forward`) aceptan el argumento `fields` para devolver
   solo los campos pedidos, y devuelven `detail_action` con los argumentos
   exactos para extraer el cuerpo completo de cada entry. Este mecanismo de
   lectura lazy **existe y funciona, pero la guía no lo presenta como práctica
   de ahorro** — la card `time` no lo menciona.
2. **Diseño**: `kmp_near` es el único que trae coordenadas dimensionales
   completas por defecto (~4× el coste de forward con fields mínimos), y
   wake/ask no aplican el patrón delta/lazy que los verbos temporales ya
   tienen.

## Evidencia (medido 2026-09-22 sobre `experiment:about-cost-probe`, 17 entries)

```
kmp_near    (default, coords completas): ~2.500 tok/llamada, 17 entries, 4 por página
kmp_rewind  fields=[ref,kind,text]     : ~700 tok, 5 entries con texto
kmp_forward fields=[ref,kind]          : ~250 tok, 5 refs puros   <- 10x más barato que near
kmp_goto    fields=[ref,kind,text]     : ~4.200 tok, 17 textos completos en UNA página
```

- El modo mínimo (`fields: ["ref","kind"]`) es 10× más barato que near por
  defecto para la misma selección.
- `detail_action` devuelve `{"tool": "kmp_rewind", "arguments": {...}}` por
  entry: el agente puede decidir qué cuerpos leer sin otro contract que el
  que ya recibió.
- goto con fields reducidos fue la lectura más eficiente por byte: 17 textos
  en una página, sin continuaciones.

## Propuesta

1. **Guía**: la card `time` añade la práctica: "navega con
   `fields: ["ref","kind"]`; pide cuerpos con los `detail_action` devueltos;
   usa near solo cuando necesites coordenadas dimensionales".
2. **Defaults**: evaluar `fields: ["ref","kind"]` como default en
   rewind/forward (donde el flujo típico es descubrir y luego leer), dejando
   el default completo en goto/near.
3. **Extender el patrón**: aplicar `detail_action` lazy a wake y ask (hoy
   empujan textos de evidencia completos en cada página — ver 02).

## Impacto esperado

- Lecturas exploratorias: −60-75% por llamada en los flujos típicos
  (descubrir refs → leer cuerpos seleccionados).
- En Astra: una ronda de navegación temporal completa baja de ~$0,08 a ~$0,03.

## Riesgos

- Bajo: `fields` ya es contract público; solo cambian defaults y guía.
- Los hosts que asumen texto completo en rewind/forward tendrían que adaptar;
  mitiga mantener `fields` explícito disponible.

## Esfuerzo

Bajo para (1) — editorial.json + regeneración de assets. Medio para (2)/(3) —
defaults y extensión del patrón a otros verbos.
