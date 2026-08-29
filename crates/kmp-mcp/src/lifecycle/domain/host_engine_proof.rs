use super::engine_proof::EngineProof;
use super::host::Host;

/// Runtime proof assigned to the host that actually consumes the executable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostEngineProof {
    host: Host,
    proof: EngineProof,
}

impl HostEngineProof {
    pub fn new(host: Host, proof: EngineProof) -> Self {
        Self { host, proof }
    }

    pub fn host(&self) -> Host {
        self.host
    }

    pub fn proof(&self) -> &EngineProof {
        &self.proof
    }
}
