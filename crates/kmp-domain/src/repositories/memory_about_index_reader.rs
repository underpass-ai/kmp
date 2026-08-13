use std::future::Future;
use std::sync::Arc;

use crate::PortError;

pub trait MemoryAboutIndexReader {
    fn list_memory_abouts(&self) -> impl Future<Output = Result<Vec<String>, PortError>> + Send;

    fn list_memory_abouts_by_dimensions<'a>(
        &'a self,
        dimension_ids: &'a [String],
    ) -> impl Future<Output = Result<Vec<String>, PortError>> + Send
    where
        Self: Sync,
    {
        async move {
            let _ = dimension_ids;
            self.list_memory_abouts().await
        }
    }
}

impl<T> MemoryAboutIndexReader for Arc<T>
where
    T: MemoryAboutIndexReader + Send + Sync + ?Sized,
{
    async fn list_memory_abouts(&self) -> Result<Vec<String>, PortError> {
        self.as_ref().list_memory_abouts().await
    }

    async fn list_memory_abouts_by_dimensions<'a>(
        &'a self,
        dimension_ids: &'a [String],
    ) -> Result<Vec<String>, PortError> {
        self.as_ref()
            .list_memory_abouts_by_dimensions(dimension_ids)
            .await
    }
}

impl<T> MemoryAboutIndexReader for &T
where
    T: MemoryAboutIndexReader + Send + Sync + ?Sized,
{
    async fn list_memory_abouts(&self) -> Result<Vec<String>, PortError> {
        (*self).list_memory_abouts().await
    }
}
