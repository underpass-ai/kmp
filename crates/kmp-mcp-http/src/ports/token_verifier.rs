use std::future::Future;
use std::pin::Pin;

use crate::domain::auth_error::AuthError;
use crate::domain::identity::Identity;

pub type VerifyFuture<'a> = Pin<Box<dyn Future<Output = Result<Identity, AuthError>> + Send + 'a>>;

/// Outbound authentication port. HTTP does not depend on OIDC mechanics.
pub trait TokenVerifier: Send + Sync {
    fn verify<'a>(&'a self, token: &'a str) -> VerifyFuture<'a>;
}
