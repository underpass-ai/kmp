use std::future::Future;
use std::pin::Pin;

use serde_json::Value;

use crate::serving::tool_error::ToolError;

/// What a backend answers with: one boxed future per tool call, so the
/// port stays object-safe across the embedded, gRPC and fixture kernels.
pub type KernelMcpToolFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + 'a>>;
