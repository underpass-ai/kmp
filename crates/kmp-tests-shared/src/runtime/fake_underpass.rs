#![allow(dead_code)]

use std::collections::BTreeMap;
use std::error::Error;
use std::io;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

use crate::debug::debug_log_value;
use crate::runtime::workspace::ToolDescriptor;

const SESSION_ID: &str = "session-agentic-runtime";

#[derive(Clone, Default)]
struct RuntimeState {
    files: Arc<Mutex<BTreeMap<String, String>>>,
}

pub struct FakeUnderpassRuntime {
    base_url: String,
    files: Arc<Mutex<BTreeMap<String, String>>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    server_task: tokio::task::JoinHandle<Result<(), io::Error>>,
}

impl FakeUnderpassRuntime {
    pub async fn start() -> Result<Self, Box<dyn Error + Send + Sync>> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let base_url = format!("http://{addr}");
        debug_log_value("fake runtime base_url", &base_url);
        let state = RuntimeState::default();
        let files = Arc::clone(&state.files);
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let server_task =
            tokio::spawn(async move { run_server(listener, state, shutdown_rx).await });

        Ok(Self {
            base_url,
            files,
            shutdown_tx: Some(shutdown_tx),
            server_task,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn read_file(&self, path: &str) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
        let files = self
            .files
            .lock()
            .map_err(|error| io::Error::other(format!("runtime state lock poisoned: {error}")))?;
        Ok(files.get(path).cloned())
    }

    pub async fn shutdown(self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let FakeUnderpassRuntime {
            base_url: _,
            files: _,
            shutdown_tx,
            server_task,
        } = self;
        if let Some(tx) = shutdown_tx {
            let _ = tx.send(());
        }

        server_task.await??;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct InvokeToolRequest {
    args: Value,
    approved: bool,
}

#[derive(Debug, Serialize)]
struct CreateSessionResponse {
    session_id: String,
}

#[derive(Debug, Serialize)]
struct ToolsResponse {
    tools: Vec<ToolDescriptorPayload>,
}

#[derive(Debug, Serialize)]
struct ToolDescriptorPayload {
    name: String,
    requires_approval: bool,
}

#[derive(Debug, Serialize)]
struct InvokeToolResponse {
    tool_name: String,
    output: String,
}

struct RuntimeRequest {
    method: String,
    path: String,
    body: Value,
}

async fn run_server(
    listener: TcpListener,
    state: RuntimeState,
    mut shutdown_rx: oneshot::Receiver<()>,
) -> Result<(), io::Error> {
    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                let (socket, _) = accept_result?;
                handle_connection(socket, state.clone()).await?;
            }
            _ = &mut shutdown_rx => return Ok(()),
        }
    }
}

async fn handle_connection(mut socket: TcpStream, state: RuntimeState) -> Result<(), io::Error> {
    let request = read_request(&mut socket).await?;
    debug_log_value(
        "fake runtime request",
        format!("{} {}", request.method, request.path),
    );
    let (status_code, body) = route_request(&state, request);
    write_response(&mut socket, body, status_code).await
}

async fn read_request(socket: &mut TcpStream) -> Result<RuntimeRequest, io::Error> {
    let mut raw = Vec::new();
    socket.read_to_end(&mut raw).await?;
    let raw = String::from_utf8(raw)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

    let (headers, body) = raw.split_once("\r\n\r\n").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "request missing header separator",
        )
    })?;
    let request_line = headers.lines().next().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "request missing request line")
    })?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "request method missing"))?;
    let path = parts
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "request path missing"))?;
    let body = if body.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(body)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
    };

    Ok(RuntimeRequest {
        method: method.to_string(),
        path: path.to_string(),
        body,
    })
}

fn route_request(state: &RuntimeState, request: RuntimeRequest) -> (u16, Value) {
    match (request.method.as_str(), request.path.as_str()) {
        ("POST", "/v1/sessions") => (
            200,
            serde_json::to_value(CreateSessionResponse {
                session_id: SESSION_ID.to_string(),
            })
            .expect("session response should serialize"),
        ),
        ("GET", path) if path == format!("/v1/sessions/{SESSION_ID}/tools") => (
            200,
            serde_json::to_value(ToolsResponse {
                tools: supported_tools()
                    .into_iter()
                    .map(|tool| ToolDescriptorPayload {
                        name: tool.name,
                        requires_approval: tool.requires_approval,
                    })
                    .collect(),
            })
            .expect("tools response should serialize"),
        ),
        ("POST", path)
            if path.starts_with(&format!("/v1/sessions/{SESSION_ID}/tools/"))
                && path.ends_with("/invoke") =>
        {
            let tool_name = path
                .trim_start_matches(&format!("/v1/sessions/{SESSION_ID}/tools/"))
                .trim_end_matches("/invoke");
            match serde_json::from_value::<InvokeToolRequest>(request.body) {
                Ok(request) => match invoke_tool(state, tool_name, request) {
                    Ok(output) => (
                        200,
                        serde_json::to_value(InvokeToolResponse {
                            tool_name: tool_name.to_string(),
                            output,
                        })
                        .expect("invoke response should serialize"),
                    ),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        (404, json!({ "error": error.to_string() }))
                    }
                    Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                        (403, json!({ "error": error.to_string() }))
                    }
                    Err(error) => (400, json!({ "error": error.to_string() })),
                },
                Err(error) => (400, json!({ "error": error.to_string() })),
            }
        }
        _ => (404, json!({ "error": "not found" })),
    }
}

fn invoke_tool(
    state: &RuntimeState,
    tool_name: &str,
    request: InvokeToolRequest,
) -> Result<String, io::Error> {
    match tool_name {
        "fs.write" => write_file(state, &request),
        "fs.read" => read_file(state, &request),
        "fs.list" => list_files(state),
        _ => Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("unsupported tool `{tool_name}`"),
        )),
    }
}

fn supported_tools() -> Vec<ToolDescriptor> {
    vec![
        ToolDescriptor {
            name: "fs.write".to_string(),
            requires_approval: true,
        },
        ToolDescriptor {
            name: "fs.read".to_string(),
            requires_approval: false,
        },
        ToolDescriptor {
            name: "fs.list".to_string(),
            requires_approval: false,
        },
    ]
}

fn write_file(state: &RuntimeState, request: &InvokeToolRequest) -> Result<String, io::Error> {
    if !request.approved {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "fs.write requires approval",
        ));
    }

    let path = json_string_arg(&request.args, "path")?;
    let content = json_string_arg(&request.args, "content")?;
    let mut files = state
        .files
        .lock()
        .map_err(|error| io::Error::other(format!("runtime state lock poisoned: {error}")))?;
    files.insert(path.clone(), content);

    Ok(format!("wrote {path}"))
}

fn read_file(state: &RuntimeState, request: &InvokeToolRequest) -> Result<String, io::Error> {
    let path = json_string_arg(&request.args, "path")?;
    let files = state
        .files
        .lock()
        .map_err(|error| io::Error::other(format!("runtime state lock poisoned: {error}")))?;
    files
        .get(&path)
        .cloned()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("missing file `{path}`")))
}

fn list_files(state: &RuntimeState) -> Result<String, io::Error> {
    let files = state
        .files
        .lock()
        .map_err(|error| io::Error::other(format!("runtime state lock poisoned: {error}")))?;
    Ok(files.keys().cloned().collect::<Vec<_>>().join("\n"))
}

fn json_string_arg(args: &Value, key: &str) -> Result<String, io::Error> {
    args.get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("missing `{key}`")))
}

async fn write_response(
    socket: &mut TcpStream,
    body: Value,
    status_code: u16,
) -> Result<(), io::Error> {
    let body = serde_json::to_vec(&body).map_err(io::Error::other)?;
    let status_text = match status_code {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Internal Server Error",
    };
    let response = format!(
        "HTTP/1.1 {status_code} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );

    socket.write_all(response.as_bytes()).await?;
    socket.write_all(&body).await?;
    socket.shutdown().await
}
