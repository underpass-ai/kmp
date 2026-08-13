use std::error::Error;

use serde_json::Value;

pub type RuntimeResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDescriptor {
    pub name: String,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolInvocation {
    pub tool_name: String,
    pub output: String,
}

pub trait AgentRuntime {
    fn list_tools(
        &self,
    ) -> impl std::future::Future<Output = RuntimeResult<Vec<ToolDescriptor>>> + Send;

    fn invoke(
        &self,
        tool_name: &str,
        args: Value,
        approved: bool,
    ) -> impl std::future::Future<Output = RuntimeResult<ToolInvocation>> + Send;
}
