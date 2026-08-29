use serde::Serialize;

use super::engine_executable::EngineExecutable;
use super::release_version::ReleaseVersion;

/// Runtime proof that one executable starts the exact MCP contract.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EngineProof {
    executable: EngineExecutable,
    version: ReleaseVersion,
    tools: Vec<String>,
}

impl EngineProof {
    pub fn proven(
        executable: EngineExecutable,
        version: ReleaseVersion,
        tools: Vec<String>,
    ) -> Self {
        Self {
            executable,
            version,
            tools,
        }
    }

    pub fn executable(&self) -> &EngineExecutable {
        &self.executable
    }

    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }

    pub fn version(&self) -> &ReleaseVersion {
        &self.version
    }
}
