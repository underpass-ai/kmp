mod embedded;
pub(crate) mod embedded_backend;
pub(crate) mod embedded_errors;
pub(crate) mod fixture_backend;
pub(crate) mod grpc;
pub(crate) mod lexical_bridge_file;
mod loopback_semantic_retriever;
pub(crate) mod retrying_embedded_backend;
mod semantic_rank_response;
mod semantic_retriever_config;
pub(crate) mod tool_request_mapping;
#[allow(dead_code)] // consumed by kmp_curate (jev-curate plan 2)
mod typesafe_api_key;
#[allow(dead_code)] // consumed by kmp_curate (jev-curate plan 2)
mod typesafe_batches;
#[allow(dead_code)] // consumed by kmp_curate (jev-curate plan 2)
mod typesafe_config;
#[allow(dead_code)] // consumed by kmp_curate (jev-curate plan 2)
mod typesafe_request_body;
#[allow(dead_code)] // consumed by kmp_curate (jev-curate plan 2)
mod typesafe_wire_response;
