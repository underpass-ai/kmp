# Neo4j Schema Migrations

The projection graph schema is managed by an explicit operator-run migration
binary. It is not executed automatically during server startup, so production
schema changes remain auditable and gated.

Run the migration from this repository with the same `KMP_GRAPH_URI`
used by the kernel server:

```bash
cargo run -p kmp-server --bin kmp-neo4j-migrate
```

The migration is idempotent. It ensures:

- `ProjectionNode(node_id)` unique constraint;
- `ProjectionNode(node_kind)` index;
- `ProjectionNode(status)` index;
- `ProjectionNode(observed_at)` index;
- `RELATED_TO(relation_type)` relationship property index.

Validate the applied schema from Cypher before replaying production-real
training or evaluation data:

```cypher
SHOW CONSTRAINTS
YIELD name, type, labelsOrTypes, properties
WHERE name = 'projection_node_id_unique'
RETURN name, type, labelsOrTypes, properties;

SHOW INDEXES
YIELD name, labelsOrTypes, properties
WHERE name IN [
  'projection_node_kind',
  'projection_node_status',
  'projection_node_observed_at',
  'related_to_relation_type'
]
RETURN name, labelsOrTypes, properties
ORDER BY name;
```

If the unique constraint fails, stop and inspect duplicate
`ProjectionNode.node_id` values before retraining or running live replay.
