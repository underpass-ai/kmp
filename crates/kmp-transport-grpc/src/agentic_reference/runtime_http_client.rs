use std::io;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::agentic_reference::logging::{debug_log, debug_log_value};
use crate::agentic_reference::runtime_contract::{
    AgentRuntime, RuntimeResult, ToolDescriptor, ToolInvocation,
};

#[derive(Clone)]
pub struct UnderpassRuntimeClient {
    authority: String,
    session_id: String,
}

impl UnderpassRuntimeClient {
    pub async fn connect(base_url: impl Into<String>) -> RuntimeResult<Self> {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        let authority = extract_authority(&base_url)?;
        debug_log_value("runtime client connect", &base_url);
        let response: CreateSessionResponse =
            send_json_request(&authority, "POST", "/v1/sessions", &json!({})).await?;
        debug_log_value("runtime client session_id", &response.session_id);

        Ok(Self {
            authority,
            session_id: response.session_id,
        })
    }

    fn tools_path(&self) -> String {
        format!("/v1/sessions/{}/tools", self.session_id)
    }

    fn invoke_path(&self, tool_name: &str) -> String {
        format!("{}/{tool_name}/invoke", self.tools_path())
    }
}

impl AgentRuntime for UnderpassRuntimeClient {
    async fn list_tools(&self) -> RuntimeResult<Vec<ToolDescriptor>> {
        debug_log("runtime client list_tools");
        let response: ToolsResponse =
            send_json_request(&self.authority, "GET", &self.tools_path(), &json!({})).await?;

        Ok(response
            .tools
            .into_iter()
            .map(|tool| ToolDescriptor {
                name: tool.name,
                requires_approval: tool.requires_approval,
            })
            .collect())
    }

    async fn invoke(
        &self,
        tool_name: &str,
        args: Value,
        approved: bool,
    ) -> RuntimeResult<ToolInvocation> {
        debug_log_value("runtime client invoke", tool_name);
        let response: InvokeToolResponse = send_json_request(
            &self.authority,
            "POST",
            &self.invoke_path(tool_name),
            &InvokeToolRequest { args, approved },
        )
        .await?;

        Ok(ToolInvocation {
            tool_name: response.tool_name,
            output: response.output,
        })
    }
}

#[derive(Debug, Deserialize)]
struct CreateSessionResponse {
    session_id: String,
}

#[derive(Debug, Deserialize)]
struct ToolsResponse {
    tools: Vec<ToolDescriptorResponse>,
}

#[derive(Debug, Deserialize)]
struct ToolDescriptorResponse {
    name: String,
    requires_approval: bool,
}

#[derive(Debug, Serialize)]
struct InvokeToolRequest {
    args: Value,
    approved: bool,
}

#[derive(Debug, Deserialize)]
struct InvokeToolResponse {
    tool_name: String,
    output: String,
}

fn extract_authority(base_url: &str) -> io::Result<String> {
    base_url
        .strip_prefix("http://")
        .map(ToString::to_string)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unsupported runtime base url"))
}

async fn send_json_request<T, B>(
    authority: &str,
    method: &str,
    path: &str,
    body: &B,
) -> RuntimeResult<T>
where
    T: for<'de> Deserialize<'de>,
    B: Serialize,
{
    let mut stream = TcpStream::connect(authority).await?;
    let body = serde_json::to_vec(body)?;
    debug_log_value("runtime http request", format!("{method} {path}"));
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {authority}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );

    stream.write_all(request.as_bytes()).await?;
    if !body.is_empty() {
        stream.write_all(&body).await?;
    }
    stream.shutdown().await?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response).await?;
    let response = String::from_utf8(response)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    debug_log_value("runtime http raw response bytes", response.len());

    parse_json_response(&response)
}

fn parse_json_response<T>(response: &str) -> RuntimeResult<T>
where
    T: for<'de> Deserialize<'de>,
{
    let (headers, body) = response.split_once("\r\n\r\n").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "runtime response missing body separator",
        )
    })?;
    let status_line = headers.lines().next().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "runtime response missing status line",
        )
    })?;
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "runtime status missing"))?;

    if !status_code.starts_with('2') {
        return Err(io::Error::other(format!("runtime request failed: {status_line}")).into());
    }

    Ok(serde_json::from_str(body)?)
}
