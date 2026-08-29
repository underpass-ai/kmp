use serde_json::Value;

use crate::authorization::{AuthorizationError, authorize};
use crate::domain::identity::Identity;

/// Application boundary for request authorization. HTTP adapters tell this
/// use case to authorize; they do not inspect token grants themselves.
#[derive(Clone, Copy, Debug, Default)]
pub struct AuthorizeMcpRequest;

impl AuthorizeMcpRequest {
    pub fn execute(identity: &Identity, request: &Value) -> Result<(), AuthorizationError> {
        authorize(identity, request)
    }
}
