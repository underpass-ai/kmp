//! Agent identity and delivered guidance belong to the MCP interaction, not to
//! the user's evidence graph. A delivery record never asserts understanding.
mod adapters;
mod agent_context;
mod agent_context_id;
mod agent_id;
mod agent_identity;
mod agent_open;
mod agent_session;
mod agent_use;
mod guidance_error;
mod guidance_purpose;
mod guide_request;
mod guide_scheme;
mod ports;
mod use_outcome;

pub(crate) use adapters::SqliteAgentDirectory;
pub(crate) use agent_context::AgentContext;
pub(crate) use agent_context_id::AgentContextId;
pub(crate) use agent_id::AgentId;
pub(crate) use agent_identity::AgentIdentity;
pub(crate) use agent_open::AgentOpen;
pub(crate) use agent_session::AgentSession;
pub(crate) use agent_use::AgentUse;
pub(crate) use guidance_error::GuidanceError;
pub(crate) use guidance_purpose::GuidancePurpose;
pub(crate) use guide_request::GuideRequest;
pub(crate) use guide_scheme::scheme;
pub(crate) use ports::{AgentDirectory, AgentIdentitySource};
pub(crate) use use_outcome::UseOutcome;
