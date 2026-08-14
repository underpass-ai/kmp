#![allow(dead_code)]

use std::collections::BTreeMap;
use std::io;
use std::sync::{Arc, Mutex};

use serde_json::Value;

pub use crate::runtime::agentic_reference::{
    AgentRuntime, RuntimeResult, ToolDescriptor, ToolInvocation,
};

use crate::debug::debug_log_value;

#[derive(Debug, Clone, Default)]
pub struct RecordingRuntime {
    files: Arc<Mutex<BTreeMap<String, String>>>,
    invocations: Arc<Mutex<Vec<RecordedInvocation>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedInvocation {
    pub tool_name: String,
    pub path: Option<String>,
    pub approved: bool,
}

impl RecordingRuntime {
    pub fn read_file(&self, path: &str) -> RuntimeResult<Option<String>> {
        let files = self.files.lock().map_err(|error| {
            io::Error::other(format!("recording runtime lock poisoned: {error}"))
        })?;
        Ok(files.get(path).cloned())
    }

    pub fn invocations(&self) -> RuntimeResult<Vec<RecordedInvocation>> {
        let invocations = self.invocations.lock().map_err(|error| {
            io::Error::other(format!("recording runtime lock poisoned: {error}"))
        })?;
        Ok(invocations.clone())
    }

    pub fn clear_invocations(&self) -> RuntimeResult<()> {
        let mut invocations = self.invocations.lock().map_err(|error| {
            io::Error::other(format!("recording runtime lock poisoned: {error}"))
        })?;
        invocations.clear();
        Ok(())
    }
}

impl AgentRuntime for RecordingRuntime {
    async fn list_tools(&self) -> RuntimeResult<Vec<ToolDescriptor>> {
        debug_log_value("recording runtime list_tools", "fs.write,fs.read,fs.list");
        Ok(vec![
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
        ])
    }

    async fn invoke(
        &self,
        tool_name: &str,
        args: Value,
        approved: bool,
    ) -> RuntimeResult<ToolInvocation> {
        debug_log_value("recording runtime invoke", tool_name);
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let mut invocations = self.invocations.lock().map_err(|error| {
            io::Error::other(format!("recording runtime lock poisoned: {error}"))
        })?;
        invocations.push(RecordedInvocation {
            tool_name: tool_name.to_string(),
            path,
            approved,
        });
        drop(invocations);

        match tool_name {
            "fs.write" => {
                if !approved {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "fs.write requires approval",
                    )
                    .into());
                }
                let path = json_string_arg(&args, "path")?;
                let content = json_string_arg(&args, "content")?;
                let mut files = self.files.lock().map_err(|error| {
                    io::Error::other(format!("recording runtime lock poisoned: {error}"))
                })?;
                files.insert(path.clone(), content);

                Ok(ToolInvocation {
                    tool_name: tool_name.to_string(),
                    output: format!("wrote {path}"),
                })
            }
            "fs.read" => {
                let path = json_string_arg(&args, "path")?;
                let files = self.files.lock().map_err(|error| {
                    io::Error::other(format!("recording runtime lock poisoned: {error}"))
                })?;
                let content = files.get(&path).cloned().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, format!("missing file `{path}`"))
                })?;

                Ok(ToolInvocation {
                    tool_name: tool_name.to_string(),
                    output: content,
                })
            }
            "fs.list" => {
                let files = self.files.lock().map_err(|error| {
                    io::Error::other(format!("recording runtime lock poisoned: {error}"))
                })?;
                let listing = files.keys().cloned().collect::<Vec<_>>().join("\n");

                Ok(ToolInvocation {
                    tool_name: tool_name.to_string(),
                    output: listing,
                })
            }
            _ => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("unsupported tool `{tool_name}`"),
            )
            .into()),
        }
    }
}

fn json_string_arg(args: &Value, key: &str) -> RuntimeResult<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("missing string arg `{key}`"),
            )
            .into()
        })
}
