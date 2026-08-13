use kmp_domain::{MemoryAboutIndexReader, SnapshotStore};

use crate::ApplicationError;
use crate::queries::QueryApplicationService;

impl<G, D, S> QueryApplicationService<G, D, S>
where
    G: MemoryAboutIndexReader + Send + Sync,
    S: SnapshotStore + Send + Sync,
{
    pub async fn list_memory_abouts(&self) -> Result<Vec<String>, ApplicationError> {
        Ok(self.graph_reader.list_memory_abouts().await?)
    }

    pub async fn list_memory_abouts_by_dimensions(
        &self,
        dimension_ids: &[String],
    ) -> Result<Vec<String>, ApplicationError> {
        Ok(self
            .graph_reader
            .list_memory_abouts_by_dimensions(dimension_ids)
            .await?)
    }
}
