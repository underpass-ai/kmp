//! What a bounded recall projection returns: the projected packet, or the
//! fact that its stable citation core alone exceeds the byte ceiling.

use serde_json::Value;

#[derive(Debug)]
pub enum ProjectionOutcome {
    Projected(Value),
    CoreTooLarge,
}
