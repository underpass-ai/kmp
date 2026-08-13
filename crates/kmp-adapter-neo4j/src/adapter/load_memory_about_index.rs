use kmp_ports::{MemoryAboutIndexReader, PortError};

use super::projection_store::Neo4jProjectionStore;
use super::queries::{list_memory_abouts_by_dimensions_query, list_memory_abouts_query};
use super::row_mapping::row_string;

impl MemoryAboutIndexReader for Neo4jProjectionStore {
    async fn list_memory_abouts(&self) -> Result<Vec<String>, PortError> {
        let graph = self.graph().await?;
        let rows = self
            .fetch_rows(&graph, list_memory_abouts_query(), "list memory abouts")
            .await?;

        rows.iter()
            .map(|row| row_string(row, "about", "memory about index"))
            .collect()
    }

    async fn list_memory_abouts_by_dimensions(
        &self,
        dimension_ids: &[String],
    ) -> Result<Vec<String>, PortError> {
        let graph = self.graph().await?;
        let rows = self
            .fetch_rows(
                &graph,
                list_memory_abouts_by_dimensions_query(dimension_ids),
                "list memory abouts by dimensions",
            )
            .await?;

        rows.iter()
            .map(|row| row_string(row, "about", "memory about dimension index"))
            .collect()
    }
}
