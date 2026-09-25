# 07 — Detección proactiva de guía desincronizada

## Problema

El store servía un scheme con 9 topics, pero el nodo
`guide:kmp-agent:card:condense` no existía en el store (guía del plugin 0.18.9
más nueva que el snapshot del store, kernel 0.5.2). El fallo solo se
descubrió al expandir el topic, con un error de ~150 tok que un agente sin
contexto de debugging interpretaría mal (o saltaría, o reintentaría).

## Evidencia

```
kmp_guide {"topic":"condense"} ->
  error: "The selected store cannot serve guide node `guide:kmp-agent:card:condense`..."
  + instrucciones completas de repair (~450 tok) aunque el remedio era un sync
Repuesto con: kmp-mcp guide sync -> "converged 2 immutable guide memories"
El guide_revision cambió (f61b224d -> f3744fdc): la brecha era real.
```

## Propuesta

1. En el registro de guía (`kmp_guide` sin topic), incluir un healthcheck
   barato: comparar los refs del scheme contra el store (ya está cargado) y
   añadir `"guide_health": "ok" | {"missing": ["card:condense"]}`.
2. Si hay brecha, la respuesta de registro sugiere el sync en UNA línea
   (el detalle completo de repair solo en el fallo real).
3. Opcional: `guide sync` automático al arrancar el MCP si detecta brecha
   (es idempotente y local).

## Impacto esperado

- Directo: casi nulo en tokens (el healthcheck son ~20 tok).
- Indirecto: evita el ciclo error→diagnóstico→sync→reintento que en la sesión
  real costó ~600 tok y una llamada desperdiciada. En agentes no supervisados,
  evita abandonos de tarea.
- Fiabilidad: la guía es la fuente de verdad del contrato para el agente;
  servirla incompleta es una fuente silenciosa de mal uso de los verbos.

## Riesgos

- Mínimo. El check es local, sobre nodos ya en memoria.

## Esfuerzo

Bajo. Un recorrido de refs del scheme contra el store en el registro.
