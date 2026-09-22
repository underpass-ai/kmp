# 09 — Las relaciones ricas se pagan a sí mismas

## Problema (planteado como hipótesis del experimento)

Escribir relaciones ricas (`connect_to` con `why`, `evidence`, `confidence`)
cuesta más en el write: cada arista añade ~60-80 tokens de payload y el packet
pasa por revisión de vecindario (`needs_review` + resume). La hipótesis a
verificar: ¿esas relaciones hacen que las lecturas posteriores cuesten menos,
suficiente para compensar?

## Evidencia (medido 2026-09-22, about `experiment:about-cost-probe`, 17 entries)

**Coste de escribir ricas:**
```
7 evidencias + decision imp-map con 5 aristas verified_by:
  payload ~1.400 tok + receipt ~700 + review ~800 + resume ~150
  vs el mismo contenido sin aristas: ~900 tok menos
Sobreprecio del enriquecimiento: ~900-1.000 tok (≈ $0,009 en Astra)
```

**Ahorro en lecturas:**
```
Trace to 3 destinos (fase-1..3) con aristas ricas:
  1 página, ~2.800 tok, rutas con why+evidence inline, 3/3 alcanzados
  El mismo contexto por wake (sin aristas): ~19.000 tok en 6 páginas
  -> 6.8x más barato

Ask compuesto (2 temas) sin ir por relaciones: 4 páginas, ~10.000 tok
  El trace con proof:true materializó lo esencial en 1 llamada

Trace con proof:true + max_body_record_bytes:
  8 cuerpos (1.345 B) con presupuesto explícito; lo que no cupo,
  defer con requisito exacto (rerun_record_bytes: 4455)
  -> el patrón delta/lazy ya existe en trace y funciona
```

**Conclusión cuantificada:** el sobreprecio de escritura (~$0,01) se amortiza
en la PRIMERA lectura posterior (~$0,19 ahorrados por trace vs wake). Ratio
~19:1 a favor, con una sola lectura de retorno. Con N lecturas, el ratio crece.

## Condiciones para que se pague

1. Las relaciones deben ser semánticamente correctas (`verified_by`,
   `chosen_because`, `depends_on`) — una arista anémica (`follows`) no ahorra:
   el lector tiene que leer los cuerpos igual para juzgar.
2. Cada arista rica debe llevar su why y evidence propios (eso es lo que
   elimina la necesidad de leer los nodos).
3. La consulta debe poder expresarse como recorrido (`trace to/seek`), no como
   pregunta semántica abierta.

## El contra-caso: `kmp_relate` a escala

```
relate de 2 abouts (probe 17 entries + project:kmp):
  287 facts, 105 declaradas, 9 propuestas, 401 items
  ~4 items/página -> ~100 páginas para agotarlo
  tensions y proposed (lo más valioso) van AL FINAL de la paginación
  project:kmp completo (1.270 entries) sería inabordable
```

`relate` con relaciones ricas entre abouts necesita:
- paginación invertida o seleccionable (tensions/proposed primero),
- o el material selection ya registrado en project:kmp.

## Propuestas

1. **Documentar el patrón en la guía** (card write): "enriquece las aristas
   cuyo recorrido planeas trazar; rich links cuestan una vez y ahorran en
   cada lectura". Cuantificar en la card: ~19:1 con una lectura.
2. **Métrica de calidad de grafo en el wake**: si el about tiene aristas
   ricas alcanzables, sugerir trace en `next_actions` ("este objetivo es
   alcanzable por trace desde X") en vez del wake genérico.
3. **Relate seleccionable**: expose `sections` o `focus: [tensions, proposed]`
   para no paginar 100 páginas de facts.

## Impacto esperado

- No reduce tokens por sí solo: **desplaza gasto de lectura a escritura**, con
  retorno neto positivo en any flujos con re-lectura (que son los de KMP).
- Combinado con 02 (delta) y 08 (fields): el trace con aristas ricas es la
  lectura más barata del sistema (~2.800 tok para lo que costaba 19.000).

## Riesgos

- Enriquecer sin criterio (todo con todo) infla el write y el grafo; el guide
  debe acotar: solo aristas con evidence real.
- La revisión de vecindario (`needs_review`) añade una llamada por packet con
  aristas ricas; con resume es barata (~150 tok) pero es fricción.

## Esfuerzo

Bajo (1): editorial. Medio (2): heuristic de next_action. Medio-alto (3):
contract de relate.
