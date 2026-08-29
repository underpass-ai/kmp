use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

use crate::application::dto::guide_request_document_dto::GuideRequestDocumentDto;
use crate::application::dto::guide_tool_call_dto::GuideToolCallDto;
use crate::application::dto::guide_tool_dto::GuideToolDto;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::ports::guide_engine::GuideEngine;

pub struct KmpBinaryGuideEngine {
    binary: PathBuf,
}

impl KmpBinaryGuideEngine {
    pub fn new(binary: impl Into<PathBuf>) -> Result<Self, ReleaseError> {
        let binary = binary.into();
        if !binary.is_file() {
            return Err(ReleaseError::invalid(format!(
                "guide engine binary does not exist: {}",
                binary.display()
            )));
        }
        Ok(Self { binary })
    }

    fn initialize(identifier: u64) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": identifier,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "kmp-guide", "version": "1"}
            }
        })
    }

    fn exchange(
        &self,
        messages: &[Value],
        data_dir: Option<&Path>,
    ) -> Result<BTreeMap<u64, Value>, ReleaseError> {
        let mut command = Command::new(&self.binary);
        command
            .env("KMP_VIEWER_ADDR", "off")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(data_dir) = data_dir {
            command
                .env("KMP_MCP_DATA_DIR", data_dir)
                .env("XDG_CONFIG_HOME", data_dir.join("config"));
        }
        let mut child = command.spawn().map_err(|error| {
            ReleaseError::invalid(format!(
                "could not start guide engine {}: {error}",
                self.binary.display()
            ))
        })?;
        let mut payload = Vec::new();
        for message in messages {
            serde_json::to_writer(&mut payload, message).map_err(|error| {
                ReleaseError::invalid(format!("could not encode MCP guide request: {error}"))
            })?;
            payload.push(b'\n');
        }
        child
            .stdin
            .take()
            .ok_or_else(|| ReleaseError::invalid("guide engine stdin was not piped"))?
            .write_all(&payload)
            .map_err(|error| {
                ReleaseError::invalid(format!("could not write guide requests: {error}"))
            })?;
        let output = child.wait_with_output().map_err(|error| {
            ReleaseError::invalid(format!("could not wait for guide engine: {error}"))
        })?;
        if !output.status.success() {
            return Err(ReleaseError::invalid(format!(
                "{} exited {}: {}",
                self.binary.display(),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        let mut responses = BTreeMap::new();
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if line.trim().is_empty() {
                continue;
            }
            let message: Value = serde_json::from_str(line).map_err(|error| {
                ReleaseError::invalid(format!(
                    "guide engine emitted invalid JSON: {error}: {line:?}"
                ))
            })?;
            if let Some(identifier) = message["id"].as_u64() {
                responses.insert(identifier, message);
            }
        }
        Ok(responses)
    }

    fn run_cli(&self, arguments: &[&str], data_dir: &Path) -> Result<(), ReleaseError> {
        let output = Command::new(&self.binary)
            .args(arguments)
            .env("KMP_VIEWER_ADDR", "off")
            .env("KMP_MCP_DATA_DIR", data_dir)
            .env("XDG_CONFIG_HOME", data_dir.join("config"))
            .output()
            .map_err(|error| {
                ReleaseError::invalid(format!("could not run guide engine: {error}"))
            })?;
        if output.status.success() {
            Ok(())
        } else {
            Err(ReleaseError::invalid(format!(
                "guide engine command failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )))
        }
    }

    fn structured_results(
        responses: &BTreeMap<u64, Value>,
        count: usize,
    ) -> Result<Vec<Value>, ReleaseError> {
        let mut results = Vec::new();
        for identifier in 2..u64::try_from(count + 2).unwrap_or(u64::MAX) {
            let response = responses.get(&identifier).ok_or_else(|| {
                ReleaseError::invalid(format!("guide engine omitted response {identifier}"))
            })?;
            if response["result"]["isError"] != false {
                return Err(ReleaseError::invalid(format!(
                    "guide tool call {identifier} failed: {response}"
                )));
            }
            results.push(response["result"]["structuredContent"].clone());
        }
        Ok(results)
    }
}

impl GuideEngine for KmpBinaryGuideEngine {
    fn version(&self) -> Result<ReleaseVersion, ReleaseError> {
        let output = Command::new(&self.binary)
            .arg("--version")
            .output()
            .map_err(|error| {
                ReleaseError::invalid(format!("cannot read engine version: {error}"))
            })?;
        let text = String::from_utf8_lossy(&output.stdout);
        let mut fields = text.split_whitespace();
        if !output.status.success() || fields.next() != Some("kmp-mcp") {
            return Err(ReleaseError::invalid(format!(
                "cannot read KMP version from {}: {}",
                self.binary.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        ReleaseVersion::parse(fields.next().unwrap_or_default())
    }

    fn live_tools(&self, data_dir: &Path) -> Result<Vec<GuideToolDto>, ReleaseError> {
        let responses = self.exchange(
            &[
                Self::initialize(1),
                json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
            ],
            Some(data_dir),
        )?;
        let tools = responses
            .get(&2)
            .and_then(|response| response["result"]["tools"].as_array())
            .ok_or_else(|| ReleaseError::invalid("tools/list did not return a tool inventory"))?;
        let mut mapped = tools
            .iter()
            .map(|tool| {
                let name = tool["name"].as_str().unwrap_or_default().to_string();
                let description = tool["description"].as_str().unwrap_or_default().to_string();
                if name.is_empty() || description.trim().is_empty() {
                    return Err(ReleaseError::invalid(
                        "tools/list returned a malformed tool definition",
                    ));
                }
                Ok(GuideToolDto { name, description })
            })
            .collect::<Result<Vec<_>, _>>()?;
        mapped.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(mapped)
    }

    fn ingest(
        &self,
        requests: &[GuideRequestDocumentDto],
        data_dir: Option<&Path>,
    ) -> Result<(), ReleaseError> {
        let mut messages = vec![Self::initialize(1)];
        for (index, request) in requests.iter().enumerate() {
            messages.push(json!({
                "jsonrpc": "2.0",
                "id": index + 2,
                "method": "tools/call",
                "params": {"name": "kmp_ingest", "arguments": request.body},
            }));
        }
        let responses = self.exchange(&messages, data_dir)?;
        let _ = Self::structured_results(&responses, requests.len())?;
        Ok(())
    }

    fn export(&self, data_dir: &Path, destination: &Path) -> Result<(), ReleaseError> {
        let destination = destination.to_string_lossy();
        self.run_cli(
            &[
                "export",
                &destination,
                "--about",
                "guide:kmp-agent",
                "--about",
                "guide:kmp",
            ],
            data_dir,
        )
    }

    fn import(&self, data_dir: &Path, bundle: &Path) -> Result<(), ReleaseError> {
        self.run_cli(&["import", &bundle.to_string_lossy()], data_dir)
    }

    fn call_tools(
        &self,
        data_dir: &Path,
        calls: &[GuideToolCallDto],
    ) -> Result<Vec<Value>, ReleaseError> {
        let mut messages = vec![Self::initialize(1)];
        for (index, call) in calls.iter().enumerate() {
            messages.push(json!({
                "jsonrpc": "2.0",
                "id": index + 2,
                "method": "tools/call",
                "params": {"name": call.name, "arguments": call.arguments},
            }));
        }
        let responses = self.exchange(&messages, Some(data_dir))?;
        Self::structured_results(&responses, calls.len())
    }
}
