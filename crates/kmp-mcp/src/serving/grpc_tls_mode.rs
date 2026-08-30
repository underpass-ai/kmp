/// How the gRPC channel is secured, named stably for logs and metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KernelMcpGrpcTlsMode {
    Disabled,
    Server,
    Mutual,
}

impl KernelMcpGrpcTlsMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Server => "server",
            Self::Mutual => "mutual",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::KernelMcpGrpcTlsMode;

    #[test]
    fn tls_mode_names_are_stable_for_logs_and_metadata() {
        assert_eq!(KernelMcpGrpcTlsMode::Disabled.as_str(), "disabled");
        assert_eq!(KernelMcpGrpcTlsMode::Server.as_str(), "server");
        assert_eq!(KernelMcpGrpcTlsMode::Mutual.as_str(), "mutual");
    }
}
