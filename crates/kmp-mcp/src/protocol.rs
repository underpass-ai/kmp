//! The MCP wire boundary.
//!
//! One concept per module below, and this file is only the seam that names
//! them and says what leaves the boundary:
//!
//! - `session` — the handshake and the names the server answers to;
//! - `registry` — which tools this build advertises, and in what order;
//! - `tools` — one module per advertised tool, each owning its own contract;
//! - `schema`, `request_shape`, `response_shape` — the shapes tools share;
//! - `relation_vocabulary` — the writer catalog, projected from the domain;
//! - `validator` — the strictness the published schemas already declare;
//! - `json_rpc`, `result` — how an answer is framed for the transport;
//! - `chronoloom_app` — the one UI resource this server publishes.

mod chronoloom_app;
mod json_rpc;
mod registry;
mod relation_vocabulary;
mod request_shape;
mod response_shape;
mod result;
mod schema;
mod session;
mod tools;
mod validator;

pub(crate) use chronoloom_app::{
    CHRONOLOOM_APP_URI, MCP_APP_MIME, resource_read_result, resources_list_result,
};
pub(crate) use json_rpc::{jsonrpc_error, jsonrpc_result};
pub(crate) use registry::{declared_tool_names, tools_list_result, tools_list_result_with_apps};
pub(crate) use result::{app_data_success_result, tool_error_result, tool_success_result};
pub(crate) use session::{canonical_tool_name, initialize_result_with_apps};
pub(crate) use validator::reject_unknown_arguments;
