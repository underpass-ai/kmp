//! Explicit mappers between the wire DTOs and the domain. Nothing else in
//! the crate converts between the two worlds.

mod intent_digest_mapper;
mod view_intent_mapper;
mod view_state_mapper;

pub use intent_digest_mapper::logical_digest;
pub use view_intent_mapper::view_patch_from_intent;
pub use view_state_mapper::view_state_dto;
