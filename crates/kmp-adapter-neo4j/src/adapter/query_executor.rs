use kmp_ports::PortError;
use neo4rs::{Graph, Query, Row};

use super::projection_store::Neo4jProjectionStore;

impl Neo4jProjectionStore {
    pub(crate) async fn run_query(
        &self,
        graph: &Graph,
        query: Query,
        operation: &str,
    ) -> Result<(), PortError> {
        graph
            .run(query)
            .await
            .map_err(|error| PortError::Unavailable(format!("neo4j {operation} failed: {error}")))
    }

    pub(crate) async fn fetch_optional_row(
        &self,
        graph: &Graph,
        query: Query,
        operation: &str,
    ) -> Result<Option<Row>, PortError> {
        let mut rows = graph.execute(query).await.map_err(|error| {
            PortError::Unavailable(format!("neo4j {operation} failed: {error}"))
        })?;

        rows.next().await.map_err(|error| {
            PortError::Unavailable(format!("neo4j {operation} stream failed: {error}"))
        })
    }

    pub(crate) async fn fetch_rows(
        &self,
        graph: &Graph,
        query: Query,
        operation: &str,
    ) -> Result<Vec<Row>, PortError> {
        let mut rows = graph.execute(query).await.map_err(|error| {
            PortError::Unavailable(format!("neo4j {operation} failed: {error}"))
        })?;

        let mut collected = Vec::new();
        while let Some(row) = rows.next().await.map_err(|error| {
            PortError::Unavailable(format!("neo4j {operation} stream failed: {error}"))
        })? {
            collected.push(row);
        }

        Ok(collected)
    }
}
