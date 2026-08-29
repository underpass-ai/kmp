use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginEngineRole {
    Bundled,
    Path,
}

impl Display for PluginEngineRole {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bundled => formatter.write_str("cache"),
            Self::Path => formatter.write_str("PATH"),
        }
    }
}
