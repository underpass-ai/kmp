use kmp_domain::{PortError, ProjectionMutation, ProjectionWriter};

#[derive(Debug)]
pub struct RoutingProjectionWriter<G, D> {
    graph_writer: G,
    detail_writer: D,
}

impl<G, D> RoutingProjectionWriter<G, D>
where
    G: ProjectionWriter + Send + Sync,
    D: ProjectionWriter + Send + Sync,
{
    pub fn new(graph_writer: G, detail_writer: D) -> Self {
        Self {
            graph_writer,
            detail_writer,
        }
    }
}

impl<G, D> ProjectionWriter for RoutingProjectionWriter<G, D>
where
    G: ProjectionWriter + Send + Sync,
    D: ProjectionWriter + Send + Sync,
{
    async fn apply_mutations(&self, mutations: Vec<ProjectionMutation>) -> Result<(), PortError> {
        let (graph_mutations, detail_mutations): (Vec<_>, Vec<_>) = mutations
            .into_iter()
            .partition(|mutation| !matches!(mutation, ProjectionMutation::UpsertNodeDetail(_)));

        if !graph_mutations.is_empty() {
            self.graph_writer.apply_mutations(graph_mutations).await?;
        }
        if !detail_mutations.is_empty() {
            self.detail_writer.apply_mutations(detail_mutations).await?;
        }

        Ok(())
    }
}
