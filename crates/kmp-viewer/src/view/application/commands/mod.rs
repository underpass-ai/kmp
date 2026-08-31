//! The commands a boundary assembles to drive the use cases. Unlike the
//! wire DTOs next door they never serialize; they exist so a use case's
//! whole input has a name and a shape.

mod apply_intent_command;
mod open_view_command;

pub use apply_intent_command::ApplyIntentCommand;
pub use open_view_command::OpenViewCommand;
