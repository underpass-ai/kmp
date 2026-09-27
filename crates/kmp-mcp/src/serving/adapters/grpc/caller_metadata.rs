//! The MCP host a gRPC call is made for, as request metadata: the kernel
//! logs it as the caller of its Ask and Wake lines instead of this server's
//! `user-agent`. Name and version only, already bounded by the session.

use tonic::metadata::{AsciiMetadataValue, MetadataValue};
use tonic::service::Interceptor;
use tonic::{Request, Status};

#[derive(Clone, Debug, Default)]
pub(super) struct CallerMetadata {
    name: Option<AsciiMetadataValue>,
    version: Option<AsciiMetadataValue>,
}

impl CallerMetadata {
    pub(super) fn new(caller: Option<(&str, &str)>) -> Self {
        let value = |text: &str| {
            (!text.is_empty())
                .then(|| MetadataValue::try_from(text).ok())
                .flatten()
        };
        match caller {
            Some((name, version)) => Self {
                name: value(name),
                version: value(version),
            },
            None => Self::default(),
        }
    }
}

impl Interceptor for CallerMetadata {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        if let Some(name) = &self.name {
            request
                .metadata_mut()
                .insert("kmp-client-name", name.clone());
            if let Some(version) = &self.version {
                request
                    .metadata_mut()
                    .insert("kmp-client-version", version.clone());
            }
        }
        Ok(request)
    }
}

#[cfg(test)]
mod tests {
    use tonic::Request;
    use tonic::service::Interceptor;

    use super::CallerMetadata;

    #[test]
    fn a_named_caller_travels_as_metadata_and_none_adds_nothing() {
        let mut named = CallerMetadata::new(Some(("codex-mcp-client", "0.154.0")));
        let request = named.call(Request::new(())).expect("request");
        assert_eq!(
            request
                .metadata()
                .get("kmp-client-name")
                .expect("kmp-client-name"),
            "codex-mcp-client"
        );
        assert_eq!(
            request
                .metadata()
                .get("kmp-client-version")
                .expect("kmp-client-version"),
            "0.154.0"
        );

        let mut unversioned = CallerMetadata::new(Some(("codex", "")));
        let request = unversioned.call(Request::new(())).expect("request");
        assert_eq!(
            request
                .metadata()
                .get("kmp-client-name")
                .expect("kmp-client-name"),
            "codex"
        );
        assert!(request.metadata().get("kmp-client-version").is_none());

        let mut none = CallerMetadata::new(None);
        let request = none.call(Request::new(())).expect("request");
        assert!(request.metadata().get("kmp-client-name").is_none());
    }
}
