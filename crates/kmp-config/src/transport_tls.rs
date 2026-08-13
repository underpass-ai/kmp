use std::io;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransportTlsMode {
    #[default]
    Disabled,
    Server,
    Mutual,
}

impl TransportTlsMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Server => "server",
            Self::Mutual => "mutual",
        }
    }
}

pub(crate) fn parse_transport_tls_mode(key: &str, value: &str) -> io::Result<TransportTlsMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "disabled" | "plaintext" => Ok(TransportTlsMode::Disabled),
        "server" | "tls" => Ok(TransportTlsMode::Server),
        "mutual" | "mtls" => Ok(TransportTlsMode::Mutual),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported {key} `{value}`; expected one of disabled, server, mutual"),
        )),
    }
}

pub(crate) fn lookup_optional_path<F>(lookup: &F, key: &str) -> Option<PathBuf>
where
    F: Fn(&str) -> Option<String>,
{
    lookup(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}
