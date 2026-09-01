//! The application ring: commands and wire DTOs at the boundary, explicit
//! mappers between them and the domain, and one use case per operation.

mod applied;
pub mod commands;
pub mod dto;
pub mod mappers;
pub mod use_cases;

pub use applied::Applied;
