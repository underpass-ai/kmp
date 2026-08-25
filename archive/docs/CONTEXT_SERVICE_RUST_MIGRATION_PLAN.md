## RFC: Migración de `services/context` a Rust

- **Status**: Draft
- **Owner**: TBD
- **Target service**: `services/context` (Context Service, “The Brain”)
- **Drivers**: p99 latency, memory stability, concurrency safety, operational robustness
- **Non-goals**: re-diseñar el dominio, cambiar contratos públicos, cambiar Neo4j/Valkey/NATS

---

## 1) Contexto y motivación

El **Context Service** está en el “hot path” de la plataforma: rehidrata sesiones y construye prompts “surgical” para agentes. Es un buen candidato para Rust cuando:

- La **latencia p95/p99** y el throughput son críticos.
- El workload combina IO (Neo4j/Valkey/NATS) + CPU (ensamblado/serialización/detección de scopes) con alta concurrencia.
- Queremos reducir riesgo de OOM/GC pressure y mejorar predictibilidad.

**Riesgo clave**: un rewrite “big bang” rompe contratos y acopla el roadmap a tooling/CI multi-runtime. Por eso esta migración debe ser **por contrato** y con estrategia **strangler**.

---

## 2) Alcance

### In scope

- Nueva implementación Rust del Context Service manteniendo:
  - gRPC contract: `specs/fleet/context/v1/context.proto`
  - Subjects y flujos NATS descritos en `services/context/README.md`
  - Semántica observable (respuestas, errores, idempotencia, versionado)
- Observabilidad (logs estructurados + métricas + trazas) equivalente o superior.
- Rollout seguro (dual-run + canary + rollback).

### Out of scope (por ahora)

- Cambios de modelo en `core/context` (dominio) que impliquen “redefinir” agregados.
- Cambiar almacenamiento (Neo4j/Valkey) o mensajería (NATS JetStream).
- Optimizar prompts/heurísticas “de producto” (eso va en iteraciones posteriores).

---

## 3) Estado actual (baseline verificable)

### 3.1 Contratos públicos

- **gRPC**: descrito en `services/context/README.md` y generado desde `specs/fleet/context/v1/context.proto`.
  - Endpoint gRPC actual: `GRPC_PORT` (default 50054).
  - Métodos visibles: ver **Apéndice A** (extraído de `context.proto`).

- **NATS (JetStream + subjects)** (según `services/context/README.md`):
  - Inbound (Planning):
    - `planning.story.created`
    - `planning.story.transitioned`
    - `planning.task.created`
    - `planning.project.created`
    - `planning.epic.created`
    - `planning.plan.approved`
  - Inbound (Orchestration): `orchestration.deliberation.>`
  - Inbound (Async RPC-like):
    - `context.update.request`
    - `context.rehydrate.request`
  - Outbound:
    - `context.events.updated`
    - `context.update.response`
    - `context.rehydrate.response`

### 3.2 Dependencias y wiring (a revisar en detalle)

- Neo4j (query + command)
- Valkey/Redis (planning read, state cache)
- NATS JetStream (consumers y async handlers)

**Nota técnica**: `services/context/generate-protos.sh` genera Python stubs bajo `services/context/gen/` a partir de `specs/fleet/context/v1/context.proto`. La migración a Rust debe mantener el principio de **no commitear código generado** (los stubs se generan en build/runtime).

### 3.3 Comportamientos a decidir (preservar vs endurecer)

Durante la migración, es crítico explicitar qué comportamientos actuales son “contrato” y cuáles son “deuda”:

- **Fail-fast de configuración**: el servicio debe fallar si faltan credenciales/URLs críticas. (Objetivo: sí.)
- **Scopes config**: decidir si ausencia de `prompt_scopes.yaml` es error fatal o se permite “config vacía”. (Recomendado: fail-fast para producción; compatibilidad temporal en entornos de dev si se justifica y se mide.)
- **`ContextChange.payload`**: hoy es `string` con JSON. Debe preservarse como **opaco** en el contrato v1; cualquier parseo/tipado debe ocurrir internamente sin romper compatibilidad.

---

## 4) Objetivos (SLOs y criterios de éxito)

### 4.1 Objetivos funcionales (no regresión)

- **Compatibilidad total de contrato**:
  - Misma proto (o evolución compatible) para gRPC.
  - Misma semántica de subjects NATS y payloads.
- **Idempotencia**:
  - Consumidores JetStream: reentrega no puede duplicar escritura en Neo4j ni romper invariantes.
- **Errores deterministas**:
  - INVALID_ARGUMENT vs INTERNAL coherentes con el servicio actual.

### 4.2 Objetivos no funcionales

- **Latency**: mejorar p99 de `GetContext` / `RehydrateSession` en escenarios de carga.
- **Memory**: estabilidad bajo concurrencia (sin picos que provoquen OOMKill).
- **Operabilidad**:
  - Trazas end-to-end (NATS + gRPC).
  - Métricas por método y por consumer.
  - Logs estructurados con correlation-id.

---

## 5) Enfoque recomendado: Strangler + dual-run

### 5.1 Principio

No sustituir el servicio de golpe. Implementar Rust en paralelo y mover tráfico progresivamente.

### 5.2 Fases

#### Phase 0 — Inventario y “contract freeze”

- Congelar `specs/fleet/context/v1/context.proto` para la ventana de migración (solo cambios backward compatible).
- Enumerar y versionar:
  - Subjects NATS + payload schema por subject.
  - Semántica de errores por RPC.
  - Timeouts/retries esperados.

**Gate**: golden tests de contrato (ver sección 9).

#### Phase 1 — Skeleton Rust service (sin negocio)

- gRPC server en Rust que expone los mismos RPCs (stub) y responde “UNIMPLEMENTED” donde aplique.
- Health/readiness endpoints (o equivalente) para Kubernetes.
- OpenTelemetry + logging estructurado.

**Gate**: deploy en K8s sin tráfico, smoke tests.

#### Phase 2 — Read path (GetContext / RehydrateSession / ValidateScope)

- Implementar read path en Rust con:
  - Driver Neo4j / consultas equivalentes.
  - Cliente Valkey/Redis.
  - Ensamblado de prompt blocks y scopes.

**Gate**: shadow traffic (duplicar requests desde gateway/sidecar) y comparar respuestas.

#### Phase 3 — Async NATS handlers (rehydrate/update request-response)

- Replicar `context.rehydrate.request` y `context.update.request`.
- Asegurar correlación request/response y deduplicación por message-id.

**Gate**: pruebas de reentrega JetStream (al menos once delivery).

#### Phase 4 — Write path (UpdateContext y mutaciones)

- Implementar writes a Neo4j con idempotencia.
- Publicación de `context.events.updated` y versiones.

**Gate**: dual-run con “single writer” (solo uno escribe) y validación.

#### Phase 5 — Cutover

- Canary (1–5% → 25% → 50% → 100%).
- Rollback instantáneo por feature flag / routing.

---

## 6) Diseño hexagonal en Rust (propuesta)

### 6.1 Capas

- **Domain**: entidades/value objects/policies puras.
- **Application**: use cases (rehydration, scope validation, graph relationship query, context change processing).
- **Ports**:
  - `GraphQueryPort` / `GraphCommandPort`
  - `PlanningReadPort` (Valkey)
  - `MessagingPort` (NATS)
  - `ClockPort` (si aplica)
- **Adapters**:
  - Neo4j adapter
  - Redis/Valkey adapter
  - NATS JetStream adapter
  - gRPC adapter

### 6.2 Mapeo

- Mantener mappers explícitos (sin reflection/auto-map) para:
  - Protobuf ↔ DTOs
  - NATS payload ↔ DTOs

---

## 7) Compatibilidad y evolución de contratos

- **Proto**: evitar breaking changes. Si se requiere:
  - introducir `context.v2` (nuevo package) y mantener v1 durante transición.
- **NATS**: mantener subjects. Cambios de payload solo con versionado explícito (por ejemplo `schema_version`).

---

## 8) Seguridad

- **Secrets**: Neo4j/Valkey/NATS credentials via Kubernetes Secrets (no en env files en git).
- **Authn/Authz**:
  - Si hoy el gRPC no autentica, documentar la brecha y plan.
  - En NATS, preferir creds/NKeys y permisos por subject.
- **Data classification**:
  - Context puede contener información sensible del repositorio; controlar logging (no loggear payloads completos).

---

## 9) Observabilidad

### 9.1 Logs

- JSON logs con:
  - `service`, `rpc_method` / `subject`, `story_id`/`case_id`, `correlation_id`, `latency_ms`, `result`.

### 9.2 Métricas

- p50/p95/p99 por RPC.
- Counters por consumer: processed, redelivered, failed, deduplicated.
- Neo4j/Redis call latency + error rate.

### 9.3 Tracing

- OpenTelemetry trace propagation:
  - gRPC metadata → span
  - NATS headers → span

---

## 10) Estrategia de pruebas (gates)

### 10.1 Contract tests (obligatorio)

- Golden tests para:
  - `GetContext` response equivalence (normalización de whitespace si aplica).
  - `ValidateScope` allowed/missing/extra determinista.
  - `RehydrateSession` stats + packs.

### 10.2 Property-based tests (recomendado)

- Scopes: para combinaciones role/phase, verificar invariantes:
  - No “extra scopes” cuando policy los prohíbe.

### 10.3 Failure mode tests

- Neo4j timeout
- Redis unavailable
- NATS backpressure
- JetStream redelivery (idempotencia)

### 10.4 Performance tests

- Benchmark `GetContext` con N escenarios:
  - story pequeño
  - story mediano
  - story grande con decisiones y relaciones

---

## 11) Plan de CI/CD

- Pipeline:
  - build Rust (release)
  - generar stubs gRPC en build (no commitear)
  - ejecutar unit tests
  - ejecutar contract tests contra Python baseline (en CI)

- Imagen:
  - multi-stage Docker build
  - distroless o slim runtime

---

## 12) Riesgos y mitigaciones

- **Divergencia de comportamiento**: mitigación con contract tests + shadow traffic.
- **Coste de drivers (Neo4j) en Rust**: evaluar opciones y compatibilidad (bolt/http) antes de implementar Phase 2.
- **Idempotencia incompleta**: diseñar dedup store (Valkey o Neo4j) por `message-id`.
- **Observabilidad degradada**: gates de métricas/trazas antes de canary.

---

## 13) Alternativas consideradas

1) Optimizar Python (sin reescribir)
- Pros: menor riesgo, mantiene tooling.
- Contras: límites de GC/latencia bajo alta concurrencia; menor control de memoria.

2) “Rust solo para prompt assembly” (micro/librería)
- Pros: reduce alcance, mejor ROI rápido.
- Contras: añade hop de red o complejidad de linking/FFI.

---

## 14) Checklist de aceptación (Definition of Done)

- [ ] gRPC v1 compatible (proto + semántica)
- [ ] Subjects NATS compatibles
- [ ] Contract tests pasan en CI
- [ ] p99 mejora o al menos no empeora con canary
- [ ] Idempotencia verificada en redelivery
- [ ] Observabilidad (logs/métricas/trazas) verificada
- [ ] Rollback probado

---

## Apéndice A — Resumen del contrato `fleet.context.v1` (source of truth: `specs/fleet/context/v1/context.proto`)

### RPCs (v1)

- `GetContext(GetContextRequest) returns (GetContextResponse)`
- `UpdateContext(UpdateContextRequest) returns (UpdateContextResponse)`
- `RehydrateSession(RehydrateSessionRequest) returns (RehydrateSessionResponse)`
- `ValidateScope(ValidateScopeRequest) returns (ValidateScopeResponse)`
- `CreateStory(CreateStoryRequest) returns (CreateStoryResponse)`
- `CreateTask(CreateTaskRequest) returns (CreateTaskResponse)`
- `AddProjectDecision(AddProjectDecisionRequest) returns (AddProjectDecisionResponse)`
- `TransitionPhase(TransitionPhaseRequest) returns (TransitionPhaseResponse)`
- `GetGraphRelationships(GetGraphRelationshipsRequest) returns (GetGraphRelationshipsResponse)`

### Campos sensibles / de compatibilidad

- `GetContextRequest.token_budget` (hint opcional): definir si se usa o se ignora; no romper clientes.
- `GetContextResponse.blocks` + `context` (string): ambos se devuelven; mantener consistencia.
- `ContextChange.payload` (string JSON): tratar como dato opaco en v1 (no cambiar tipo).
- `RehydrateSessionResponse.packs` (map<string, RoleContextPack>): mantener keys por `role`.
- `GetGraphRelationshipsRequest.depth` (clamp max=3 en implementación Python actual): si se preserva, documentar igual en Rust.

