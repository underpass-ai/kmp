//! The view bounded context: what the human and the agent are both looking
//! at, as a shared aggregate with hexagonal boundaries.
//!
//! ChronoLoom's agent control is declarative. An agent never says "move the
//! camera to x=438"; it says "focus these refs, on occurred time, with a five
//! minute window, causal and evidential relations". That intention lands in
//! this context — domain invariants at the center, ports for what the
//! application needs, DTOs and mappers at the wire, adapters at the edge —
//! so the same state is legible to a person, to an agent, and to a test.
//!
//! The layout follows
//! [`docs/architecture/chronoloom-layer-map.md`](https://github.com/underpass-ai/kmp/blob/main/docs/architecture/chronoloom-layer-map.md).

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
mod registry;

pub use registry::ViewRegistry;
