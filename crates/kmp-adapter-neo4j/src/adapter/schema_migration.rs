use kmp_ports::PortError;
use neo4rs::query;

use super::projection_store::Neo4jProjectionStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Neo4jSchemaMigration {
    name: &'static str,
    cypher: &'static str,
}

const PROJECTION_SCHEMA_MIGRATIONS: &[Neo4jSchemaMigration] = &[
    Neo4jSchemaMigration {
        name: "projection_node_id_unique",
        cypher: "
CREATE CONSTRAINT projection_node_id_unique IF NOT EXISTS
FOR (node:ProjectionNode)
REQUIRE node.node_id IS UNIQUE
        ",
    },
    Neo4jSchemaMigration {
        name: "projection_node_kind",
        cypher: "
CREATE INDEX projection_node_kind IF NOT EXISTS
FOR (node:ProjectionNode)
ON (node.node_kind)
        ",
    },
    Neo4jSchemaMigration {
        name: "projection_node_status",
        cypher: "
CREATE INDEX projection_node_status IF NOT EXISTS
FOR (node:ProjectionNode)
ON (node.status)
        ",
    },
    Neo4jSchemaMigration {
        name: "projection_node_observed_at",
        cypher: "
CREATE INDEX projection_node_observed_at IF NOT EXISTS
FOR (node:ProjectionNode)
ON (node.observed_at)
        ",
    },
    Neo4jSchemaMigration {
        name: "related_to_relation_type",
        cypher: "
CREATE INDEX related_to_relation_type IF NOT EXISTS
FOR ()-[edge:RELATED_TO]-()
ON (edge.relation_type)
        ",
    },
];

impl Neo4jProjectionStore {
    pub async fn migrate_schema(&self) -> Result<usize, PortError> {
        let graph = self.graph().await?;

        for migration in PROJECTION_SCHEMA_MIGRATIONS {
            self.run_query(
                &graph,
                query(migration.cypher),
                &format!("schema migration `{}`", migration.name),
            )
            .await?;
        }

        Ok(PROJECTION_SCHEMA_MIGRATIONS.len())
    }
}

#[cfg(test)]
mod tests {
    use super::PROJECTION_SCHEMA_MIGRATIONS;

    #[test]
    fn projection_schema_migration_names_are_stable() {
        let names = PROJECTION_SCHEMA_MIGRATIONS
            .iter()
            .map(|migration| migration.name)
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "projection_node_id_unique",
                "projection_node_kind",
                "projection_node_status",
                "projection_node_observed_at",
                "related_to_relation_type",
            ]
        );
    }

    #[test]
    fn projection_schema_migrations_are_idempotent() {
        for migration in PROJECTION_SCHEMA_MIGRATIONS {
            assert!(
                migration.cypher.contains("IF NOT EXISTS"),
                "{} must be safe to rerun",
                migration.name
            );
        }
    }

    #[test]
    fn projection_schema_migrations_cover_hot_projection_access_patterns() {
        let combined = PROJECTION_SCHEMA_MIGRATIONS
            .iter()
            .map(|migration| migration.cypher)
            .collect::<Vec<_>>()
            .join("\n");

        assert!(combined.contains("REQUIRE node.node_id IS UNIQUE"));
        assert!(combined.contains("ON (node.node_kind)"));
        assert!(combined.contains("ON (node.status)"));
        assert!(combined.contains("ON (node.observed_at)"));
        assert!(combined.contains("FOR ()-[edge:RELATED_TO]-()"));
        assert!(combined.contains("ON (edge.relation_type)"));
    }
}
