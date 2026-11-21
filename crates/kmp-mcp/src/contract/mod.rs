//! The contract bounded context: what this server advertises and
//! validates. One definition per tool, cohesive schema families, a
//! minimal registry, and the argument validator. The advertised bytes
//! are pinned against the running binary by `tool_surface_parity`.

pub(crate) mod cross_field_rule;
mod cross_field_rule_audit;
pub(crate) mod handshake;
pub(crate) mod parameter_guide;
mod parameter_guide_audit;
pub(crate) mod published_law;
mod published_law_audit;
pub(crate) mod registry;
pub(crate) mod schema;
pub(crate) mod schema_law;
mod schema_law_audit;
pub(crate) mod schema_violation;
pub(crate) mod schema_wording;
mod summaries_audit_surface;
mod surface_audit;
pub(crate) mod temporal_entry_field;
pub(crate) mod time_move;
pub(crate) mod tools;
pub(crate) mod validator;
mod validator_switches_audit;
mod view_absence_audit;
mod writer_audit;
pub(crate) mod writer_memory_kinds;

pub(crate) use handshake::{
    CHRONOLOOM_APP_URI, MCP_APP_MIME, initialize_result_with_apps, resource_read_result,
    resources_list_result,
};
pub(crate) use registry::{
    advertised_input_schemas, advertised_tools_list, declared_tool_names, tools_list_result,
    tools_list_result_with_apps, without_output_schemas,
};
pub(crate) use time_move::{TIME_TOOL, TimeMove};
pub(crate) use validator::{reject_invalid_arguments, validate_required_arguments};
