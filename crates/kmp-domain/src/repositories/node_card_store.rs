use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;

use crate::{AuthorNodeCard, NodeCard, NodeCardRejection, PortError};

pub type NodeCardWriteFuture<'a> = Pin<
    Box<dyn Future<Output = Result<Result<NodeCard, NodeCardRejection>, PortError>> + Send + 'a>,
>;

/// Writes the derived card view.
///
/// Object-safe on purpose: a kernel mounts this the way it mounts read
/// snapshots, as one optional port, so composing a backend without cards
/// stays a compile-time-free choice rather than another generic parameter on
/// every service that never touches a card.
///
/// The nested result is the contract, not an accident. The outer failure is
/// the store: it could not be read or written. The inner one is the card
/// policy refusing a write that the store could have performed — a lost race,
/// a body that moved, prose that compresses nothing. Collapsing them would
/// turn "your card is out of date" into "memory is unavailable".
pub trait NodeCardStore: Debug + Send + Sync {
    /// Reads node, body and stored card, applies the card policy and persists
    /// the result, all inside one write transaction. Implementations must not
    /// decide admission themselves.
    fn author_node_card(&self, command: AuthorNodeCard) -> NodeCardWriteFuture<'_>;
}
