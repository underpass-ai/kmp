use std::future::Future;
use std::sync::Arc;

use crate::{MemoryWriteFacts, MemoryWriteFactsRequest, PortError};

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

    /// The facts one memory write needs about its about, read point by
    /// point (DESIGN L6, write in O(delta)). `None` when the store cannot
    /// answer them this way: the caller then reads the about's
    /// neighbourhood as before.
    fn memory_write_facts<'a>(
        &'a self,
        request: &'a MemoryWriteFactsRequest,
    ) -> impl Future<Output = Result<Option<MemoryWriteFacts>, PortError>> + Send
    where
        Self: Sync,
    {
        async move {
            let _ = request;
            Ok(None)
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

    async fn memory_write_facts<'a>(
        &'a self,
        request: &'a MemoryWriteFactsRequest,
    ) -> Result<Option<MemoryWriteFacts>, PortError> {
        self.as_ref().memory_write_facts(request).await
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
