//! The contract bounded context: what this server advertises and
//! validates. One definition per tool, cohesive schema families, a
//! minimal registry, and the argument validator. The advertised bytes
//! are pinned against the running binary by `tool_surface_parity`.

pub(crate) mod handshake;
pub(crate) mod registry;
pub(crate) mod schema;
mod surface_audit;
pub(crate) mod tools;
pub(crate) mod validator;
mod writer_audit;

pub(crate) use handshake::{
    CHRONOLOOM_APP_URI, MCP_APP_MIME, canonical_tool_name, initialize_result_with_apps,
    resource_read_result, resources_list_result,
};
pub(crate) use registry::{declared_tool_names, tools_list_result, tools_list_result_with_apps};
pub(crate) use validator::{reject_unknown_arguments, validate_required_arguments};
