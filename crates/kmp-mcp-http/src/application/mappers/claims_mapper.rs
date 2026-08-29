use crate::application::dto::claims_dto::ClaimsDto;
use crate::domain::auth_error::AuthError;
use crate::domain::identity::Identity;

#[derive(Clone, Copy, Debug, Default)]
pub struct ClaimsMapper;

impl ClaimsMapper {
    pub(crate) fn to_identity(claims: ClaimsDto) -> Result<Identity, AuthError> {
        if claims.sub.trim().is_empty() {
            return Err(AuthError::Unauthorized(
                "bearer token subject is empty".to_string(),
            ));
        }
        Ok(Identity {
            subject: claims.sub,
            workspace: claims.workspace,
            scopes: claims.scope.into_set(true),
            abouts: claims.kmp_abouts.into_set(false),
            scope_ids: claims.kmp_scope_ids.into_set(false),
            ref_prefixes: claims.kmp_ref_prefixes.into_set(false),
        })
    }
}
