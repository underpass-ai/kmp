use crate::lifecycle::domain::engine_executable::EngineExecutable;
use crate::lifecycle::domain::plugin_engine_role::PluginEngineRole;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginEngineCandidate {
    executable: EngineExecutable,
    role: PluginEngineRole,
}

impl PluginEngineCandidate {
    pub fn new(executable: EngineExecutable, role: PluginEngineRole) -> Self {
        Self { executable, role }
    }

    pub fn executable(&self) -> &EngineExecutable {
        &self.executable
    }

    pub fn role(&self) -> PluginEngineRole {
        self.role
    }
}
