use crate::domain::release_version::ReleaseVersion;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum McpbTarget {
    LinuxX86_64,
    LinuxAarch64,
    MacosX86_64,
    MacosAarch64,
    WindowsX86_64,
}

impl McpbTarget {
    pub const fn all() -> [Self; 5] {
        [
            Self::LinuxX86_64,
            Self::LinuxAarch64,
            Self::MacosX86_64,
            Self::MacosAarch64,
            Self::WindowsX86_64,
        ]
    }

    pub const fn triple(self) -> &'static str {
        match self {
            Self::LinuxX86_64 => "x86_64-unknown-linux-gnu",
            Self::LinuxAarch64 => "aarch64-unknown-linux-gnu",
            Self::MacosX86_64 => "x86_64-apple-darwin",
            Self::MacosAarch64 => "aarch64-apple-darwin",
            Self::WindowsX86_64 => "x86_64-pc-windows-msvc",
        }
    }

    pub fn input_name(self, version: &ReleaseVersion) -> String {
        let suffix = if self == Self::WindowsX86_64 {
            ".exe"
        } else {
            ""
        };
        format!("kmp-mcp-{}-{}{suffix}", version.tag(), self.triple())
    }

    pub fn archive_name(self) -> String {
        let suffix = if self == Self::WindowsX86_64 {
            ".exe"
        } else {
            ""
        };
        format!("server/bin/kmp-mcp-{}{suffix}", self.triple())
    }
}
