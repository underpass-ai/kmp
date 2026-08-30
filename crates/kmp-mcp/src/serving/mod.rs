//! The serving bounded context's first residents: JSON-RPC framing and
//! tool result envelopes. The transport and backend dispatch join them
//! in their own slice.

pub(crate) mod json_rpc;
pub(crate) mod tool_result;

pub(crate) use json_rpc::{jsonrpc_error, jsonrpc_result};
pub(crate) use tool_result::{app_data_success_result, tool_error_result, tool_success_result};
