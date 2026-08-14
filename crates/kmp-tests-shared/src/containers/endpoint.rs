/// Typed container endpoint — eliminates raw string construction scattered across fixtures.
#[derive(Debug, Clone)]
pub struct ContainerEndpoint {
    pub host: String,
    pub port: u16,
}

impl ContainerEndpoint {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }

    /// `neo4j://neo4j:{password}@{host}:{port}`
    pub fn neo4j_uri(&self, password: &str) -> String {
        format!("neo4j://neo4j:{password}@{}:{}", self.host, self.port)
    }

    /// `neo4j://{host}:{port}` (no credentials, for admin operations like clear).
    pub fn neo4j_admin_uri(&self) -> String {
        format!("neo4j://{}:{}", self.host, self.port)
    }

    /// `redis://{host}:{port}?key_prefix={prefix}&ttl_seconds={ttl}`
    pub fn redis_uri(&self, prefix: &str, ttl: u32) -> String {
        format!(
            "redis://{}:{}?key_prefix={prefix}&ttl_seconds={ttl}",
            self.host, self.port
        )
    }

    /// `nats://127.0.0.1:{port}` (NATS always binds to localhost in tests).
    pub fn nats_uri(&self) -> String {
        format!("nats://127.0.0.1:{}", self.port)
    }

    pub fn authority(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

impl std::fmt::Display for ContainerEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}
