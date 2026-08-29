use crate::lifecycle::application::use_cases::converge_lifecycle::ConvergeLifecycle;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::lifecycle_receipt::LifecycleReceipt;
use crate::lifecycle::domain::lifecycle_request::LifecycleRequest;
use crate::lifecycle::ports::engine_store::EngineStore;
use crate::lifecycle::ports::host_gateway::HostGateway;
use crate::lifecycle::ports::release_repository::ReleaseRepository;

/// Use case: refresh every installed native host and prove one exact release.
pub struct UpdateKmp<'a> {
    convergence: ConvergeLifecycle<'a>,
}

impl<'a> UpdateKmp<'a> {
    pub fn new(
        hosts: &'a dyn HostGateway,
        releases: &'a dyn ReleaseRepository,
        engines: &'a dyn EngineStore,
    ) -> Self {
        Self {
            convergence: ConvergeLifecycle::new(hosts, releases, engines),
        }
    }

    pub fn execute(&self, request: LifecycleRequest) -> Result<LifecycleReceipt, LifecycleError> {
        self.convergence.execute(request)
    }
}
