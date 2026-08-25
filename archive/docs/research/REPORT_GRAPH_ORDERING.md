> **Historical analysis.** This report predates the implemented relation `sequence` ordering. Retained for context; see RELATION_EXPLANATION_MODEL.md for the current model.

# Informe: Ordenamiento de Nodos y Relaciones en el Grafo

## Estado actual

### No existe ordenamiento narrativo

El kernel **no tiene ninguna propiedad de orden** en nodos ni relaciones. El pipeline completo es:

```
graph.yaml (orden narrativo) → seed-mission → NATS events → kernel projector → Neo4j
                                                                                  ↓
TUI (DFS walk) ← GetContext response ← ordered_neighborhood() ← Cypher query (sin ORDER BY)
```

### Capa por capa

#### 1. Neo4j Schema (`upsert_relation_projection_query.rs`)

Las relaciones `RELATED_TO` ya persisten metadatos (incluido `sequence`) en `properties_json`:

```cypher
MERGE (source)-[edge:RELATED_TO {relation_type: $relation_type}]->(target)
SET edge.properties_json = $properties_json
```

El `properties_json` se serializa desde `relation.explanation.to_properties()` (que incluye `sequence`, `semantic_class`, `rationale`) y se lee de vuelta como `relation_properties_json`.

#### 2. Domain Model (`NodeRelationProjection`)

```rust
pub struct NodeRelationProjection {
    pub source_node_id: String,
    pub target_node_id: String,
    pub relation_type: String,
    pub explanation: RelationExplanation,
}
```

4 campos. El orden/`sequence` vive dentro de `explanation` (`RelationExplanation`, que expone `sequence()`/`with_sequence()`).

#### 3. Cypher Query (`load_neighborhood_query.rs`)

```cypher
MATCH (root:ProjectionNode {node_id: $root_node_id})
OPTIONAL MATCH path = (root)-[:RELATED_TO*1..{depth}]->(reachable)
...
RETURN neighbor_node_id, ..., source_node_id, target_node_id, relation_type
```

**Sin `ORDER BY`**. Neo4j devuelve filas en orden arbitrario del storage engine.

#### 4. Application Layer (`ordered_neighborhood.rs`)

El kernel SÍ ordena, pero **lexicográficamente por ID**:

```rust
fn compare_relations(left: &NodeRelationProjection, right: &NodeRelationProjection) -> Ordering {
    (&left.source_node_id, &left.target_node_id, &left.relation_type)
        .cmp(&(&right.source_node_id, &right.target_node_id, &right.relation_type))
}
```

Esto produce un orden **determinístico pero no narrativo**. Para el nodo `era:midjourney-crisis`, sus hijos se ordenan:
```
node:agent:...              (a < d < i < t)
node:decision:model-routing
node:decision:triage-priority
node:incident:cryo-cascade
node:incident:nav-drift
node:incident:power-degradation
node:task:system-validation
```

El triage acaba **después** de los agents y decisions, y **antes** de los incidents. En la narrativa, debería ser: triage → model-routing → incidents → tasks → agents.

#### 5. Proto (`common.proto`)

```protobuf
message GraphRelationship {
  string source_node_id = 1;
  string target_node_id = 2;
  string relationship_type = 3;
  map<string, string> properties = 4;  // ← existe pero no se usa para orden
}
```

El campo `properties` existe en el proto pero **no se usa en el kernel** para ningún propósito de ordenamiento.

## Consecuencia

El DFS en el TUI recorre los hijos de cada nodo en orden lexicográfico de `target_node_id`. Esto produce:
- Era III en fase 06 (correcto por casualidad — `era:midjourney` viene después de `era:deep-cruise`)
- Triage en fase ~37 (incorrecto — debería venir antes de resolver los incidentes)
- Artifacts y agents intercalados sin lógica narrativa

## Solución propuesta: `sequence` en relaciones

### Cambios necesarios

| Capa | Fichero | Cambio |
|------|---------|--------|
| **Proto** | `common.proto` | `GraphRelationship.properties` ya existe — usar `properties["sequence"]` |
| **Domain** | `node_relation_projection.rs` | Añadir `sequence: Option<u32>` |
| **Neo4j upsert** | `upsert_relation_projection_query.rs` | Añadir `sequence` al `MERGE` |
| **Neo4j query** | `load_neighborhood_query.rs` | Leer `edge.sequence` |
| **Ordering** | `ordered_neighborhood.rs` | Ordenar por `sequence` primero, fallback a lexicográfico |
| **Seed YAML** | `graph.yaml` | Añadir `sequence: N` a cada relationship |

### Diseño

```rust
// domain
pub struct NodeRelationProjection {
    pub source_node_id: String,
    pub target_node_id: String,
    pub relation_type: String,
    pub sequence: Option<u32>,  // NEW: narrative order within parent
}

// ordering (ordered_neighborhood.rs)
fn compare_relations(left: &NodeRelationProjection, right: &NodeRelationProjection) -> Ordering {
    // Sequence first (None sorts last), then lexicographic fallback
    let seq_order = left.sequence.unwrap_or(u32::MAX)
        .cmp(&right.sequence.unwrap_or(u32::MAX));
    if seq_order != Ordering::Equal {
        return seq_order;
    }
    // Fallback: lexicographic by (source, target, type)
    (&left.source_node_id, &left.target_node_id, &left.relation_type)
        .cmp(&(&right.source_node_id, &right.target_node_id, &right.relation_type))
}
```

### Ventajas

- **Retrocompatible**: `sequence` es `Option<u32>` — relaciones sin sequence siguen funcionando con el fallback lexicográfico
- **No rompe el proto**: usa `properties` map existente, o se puede añadir un campo explícito
- **El orden narrativo es dato, no lógica**: el seed YAML define el orden, el kernel lo preserva
