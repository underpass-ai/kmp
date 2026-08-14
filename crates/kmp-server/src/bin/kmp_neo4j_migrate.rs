use std::process::ExitCode;

use kmp_adapter_neo4j::Neo4jProjectionStore;

const DEFAULT_GRAPH_URI: &str = "neo4j://localhost:7687";

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(ensured) => {
            println!("neo4j projection schema migration complete: ensured {ensured} items");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("neo4j projection schema migration failed: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let graph_uri =
        std::env::var("KMP_GRAPH_URI").unwrap_or_else(|_| DEFAULT_GRAPH_URI.to_string());
    let store = Neo4jProjectionStore::new(graph_uri)?;

    Ok(store.migrate_schema().await?)
}
