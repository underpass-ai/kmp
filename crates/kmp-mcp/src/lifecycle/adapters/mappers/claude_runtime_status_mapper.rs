use crate::lifecycle::domain::host_runtime_status::HostRuntimeStatus;

/// Anti-corruption mapper for Claude Code's human-readable MCP health output.
#[derive(Clone, Copy, Debug, Default)]
pub struct ClaudeRuntimeStatusMapper;

impl ClaudeRuntimeStatusMapper {
    pub fn map(output: &str) -> HostRuntimeStatus {
        let Some(line) = output
            .lines()
            .find(|line| line.contains("plugin:kmp:memory:"))
        else {
            return HostRuntimeStatus::Missing;
        };
        let normalized = line.to_ascii_lowercase();
        if normalized.contains("connected") && !normalized.contains("failed") {
            HostRuntimeStatus::Connected
        } else if normalized.contains("pending") {
            HostRuntimeStatus::PendingApproval
        } else if normalized.contains("disabled") {
            HostRuntimeStatus::Disabled
        } else {
            HostRuntimeStatus::Failed(line.trim().to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_real_connection_failure_is_not_mistaken_for_registration() {
        let output =
            "plugin:kmp:memory: /tmp/run-embedded-mcp.sh - ✘ Failed to connect — CONNECTION_CLOSED";
        assert!(matches!(
            ClaudeRuntimeStatusMapper::map(output),
            HostRuntimeStatus::Failed(_)
        ));
    }
}
