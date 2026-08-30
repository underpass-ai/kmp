//! The kernel's own failure classification, carried out to the caller.
//! `ApplicationError` and `PortError` are typed; that knowledge used to be
//! thrown away at this boundary and reconstructed by matching English
//! words downstream. Nothing here reads the message.

use crate::serving::{ToolError, ToolErrorCode};

pub(super) fn mapping_error(status: &tonic::Status) -> ToolError {
    ToolError::invalid_argument(status.message())
}

/// Carries the kernel's own classification out to the caller.
///
/// The kernel already knows what went wrong — `ApplicationError` and
/// `PortError` are typed — and that knowledge used to be thrown away at this
/// boundary and reconstructed by matching English words further downstream.
/// Nothing here reads the message.
pub(super) fn kernel_error<'a>(
    operation: &'a str,
    about: &'a str,
) -> impl FnOnce(kmp_application::ApplicationError) -> ToolError + 'a {
    use kmp_application::ApplicationError;
    use kmp_domain::{DomainError, PortError};

    move |error| {
        let code = match &error {
            ApplicationError::RetryableConflict(reason) => {
                return ToolError::conflict(format!(
                    "embedded kernel {operation} write conflict for `{about}`: the store moved \
                     while this write was being prepared, so this attempt was not applied. It \
                     is safe to retry the same logical write with the same `idempotency_key`; \
                     if an earlier attempt landed, idempotency returns that success instead of \
                     duplicating memory. Kernel detail: {reason}"
                ));
            }
            ApplicationError::NotFound(_) => ToolErrorCode::NotFound,
            ApplicationError::Validation(_) => ToolErrorCode::InvalidArgument,
            // A domain error is an invariant the payload broke, so the caller
            // can fix it. `EmptyValue` naming a field is the clearest case.
            ApplicationError::Domain(DomainError::EmptyValue(_)) => ToolErrorCode::InvalidArgument,
            // `InvalidState` is the ambiguous one: it covers both a payload
            // the model rejects and a store that cannot serve the request.
            // It stays a backend error, because telling an agent to fix its
            // arguments when nothing about them is wrong is the failure this
            // whole change exists to remove.
            ApplicationError::Domain(DomainError::InvalidState(_)) => ToolErrorCode::BackendError,
            ApplicationError::Ports(PortError::Conflict(_)) => ToolErrorCode::Conflict,
            ApplicationError::Ports(PortError::Unavailable(_)) => ToolErrorCode::Unavailable,
            ApplicationError::Ports(PortError::InvalidState(_)) => ToolErrorCode::BackendError,
        };
        let outcome = if code == ToolErrorCode::Conflict {
            "conflict"
        } else {
            "failed"
        };
        ToolError::new(
            code,
            format!("embedded kernel {operation} {outcome} for `{about}`: {error}"),
        )
    }
}

/// Temporal selection turns a domain `InvalidState` into a caller error: the
/// temporal domain uses that variant for an unresolved/invalid cursor, and the
/// gRPC service exposes the same condition as `INVALID_ARGUMENT`. This is
/// operation-specific classification, not message matching; port/store
/// `InvalidState` remains a backend failure through `kernel_error`.
pub(super) fn temporal_error<'a>(
    operation: &'a str,
    about: &'a str,
) -> impl FnOnce(kmp_application::ApplicationError) -> ToolError + 'a {
    move |error| {
        if matches!(
            error,
            kmp_application::ApplicationError::Domain(kmp_domain::DomainError::InvalidState(_))
        ) {
            return ToolError::invalid_argument(format!(
                "embedded kernel {operation} failed for `{about}`: {error}"
            ));
        }
        kernel_error(operation, about)(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optimistic_write_conflicts_name_the_safe_retry_contract() {
        let error = kernel_error("ingest", "incident:pool-saturation")(
            kmp_application::ApplicationError::RetryableConflict(
                "expected revision 16, current is 17".to_string(),
            ),
        );

        assert_eq!(error.code, ToolErrorCode::Conflict);
        assert!(error.message.contains("write conflict"), "{error}");
        assert!(error.message.contains("attempt was not applied"), "{error}");
        assert!(error.message.contains("safe to retry"), "{error}");
        assert!(error.message.contains("same `idempotency_key`"), "{error}");
        assert!(error.message.contains("expected revision 16"), "{error}");
    }
}
