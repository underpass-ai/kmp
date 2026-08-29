#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PluginPackageTarget {
    operating_system: String,
    architecture: String,
}

impl PluginPackageTarget {
    pub fn current() -> Self {
        let operating_system = std::env::consts::OS;
        let architecture = match std::env::consts::ARCH {
            "aarch64" => "arm64",
            value => value,
        };
        Self {
            operating_system: operating_system.to_string(),
            architecture: architecture.to_string(),
        }
    }

    pub fn suffix(&self) -> String {
        format!("{}-{}", self.operating_system, self.architecture)
    }
}
