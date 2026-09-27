//! The caller of a gRPC read as it names itself: `kmp-client-name` and
//! `kmp-client-version` metadata when it sends them (the MCP gateway
//! forwarding its host, say), its `user-agent` otherwise — a name and a
//! version, nothing else, bounded.

use kmp_observability::{CLIENT_NAME_CHARS, CLIENT_VERSION_CHARS, client_label};
use tonic::metadata::MetadataMap;

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct GrpcClient {
    pub(crate) name: Option<String>,
    pub(crate) version: Option<String>,
}

impl GrpcClient {
    pub(crate) fn from_metadata(metadata: &MetadataMap) -> Self {
        let read = |key: &str, limit: usize| {
            metadata
                .get(key)
                .and_then(|value| value.to_str().ok())
                .map(|value| client_label(value, limit))
                .filter(|value| !value.is_empty())
        };
        match read("kmp-client-name", CLIENT_NAME_CHARS) {
            Some(name) => Self {
                name: Some(name),
                version: read("kmp-client-version", CLIENT_VERSION_CHARS),
            },
            None => Self {
                name: read("user-agent", CLIENT_NAME_CHARS),
                version: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use tonic::metadata::MetadataMap;

    use super::GrpcClient;

    #[test]
    fn a_declared_client_wins_over_the_user_agent() {
        let mut metadata = MetadataMap::new();
        metadata.insert("user-agent", "tonic/0.14".parse().expect("ua"));
        assert_eq!(
            GrpcClient::from_metadata(&metadata),
            GrpcClient {
                name: Some("tonic/0.14".into()),
                version: None
            }
        );
        metadata.insert("kmp-client-name", "codex-mcp-client".parse().expect("name"));
        metadata.insert("kmp-client-version", "0.154.0\t".parse().expect("version"));
        assert_eq!(
            GrpcClient::from_metadata(&metadata),
            GrpcClient {
                name: Some("codex-mcp-client".into()),
                version: Some("0.154.0".into())
            }
        );
        assert_eq!(
            GrpcClient::from_metadata(&MetadataMap::new()),
            GrpcClient::default()
        );
    }
}
