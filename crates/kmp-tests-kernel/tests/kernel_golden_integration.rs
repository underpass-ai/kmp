#![allow(deprecated)]
#![cfg(feature = "container-tests")]

mod support;

#[path = "kernel_golden/get_context.rs"]
mod get_context;
#[path = "kernel_golden/rehydrate_session.rs"]
mod rehydrate_session;
#[path = "kernel_golden/update_context.rs"]
mod update_context;
#[path = "kernel_golden/validate_scope.rs"]
mod validate_scope;
