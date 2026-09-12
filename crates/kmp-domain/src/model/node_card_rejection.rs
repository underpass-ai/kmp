use std::fmt;

/// Why a card write was refused, with the numbers a reader needs to recover.
///
/// Every variant carries what the store actually holds, not just that the
/// caller was wrong: a refused reader can re-read the named revision and
/// author again without a second diagnostic round trip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeCardRejection {
    /// The ref does not resolve to an entry owned by this about.
    UnknownRef { node_id: String, about: String },
    /// The entry exists but the store holds no body to summarize.
    NoBody { node_id: String },
    /// The body moved between the reader's read and this write.
    SourceMoved {
        declared_revision: u64,
        actual_revision: u64,
        declared_record_digest: String,
        actual_record_digest: String,
    },
    /// The write stands at a historical cut and the card would be stamped
    /// after it.
    AuthoredAfterCut { authored_at: String },
    /// A card record is itself read on every compact response; an unbounded
    /// one would reintroduce the cost it exists to remove.
    CardTooLarge { card_bytes: usize, limit: usize },
    /// The writer declared no card exists, and one does.
    CardAlreadyExists { actual_card_revision: u64 },
    /// The writer declared a card revision, and the store holds another.
    CardMoved {
        declared_card_revision: u64,
        actual_card_revision: u64,
    },
    /// The writer declared a card revision, and no card is stored.
    CardAbsent { declared_card_revision: u64 },
    /// A card with no text compresses nothing and states nothing.
    EmptyCard,
    /// A card at least as long as the body it replaces cannot make a read
    /// cheaper. This is a floor on cost, not a judgement of the prose.
    NotCompact { card_bytes: usize, body_bytes: u64 },
}

impl NodeCardRejection {
    /// Whether this refusal is a lost race rather than a malformed request.
    /// A conflict is worth retrying after re-reading; the others are not.
    pub const fn is_conflict(&self) -> bool {
        matches!(
            self,
            NodeCardRejection::SourceMoved { .. }
                | NodeCardRejection::CardAlreadyExists { .. }
                | NodeCardRejection::CardMoved { .. }
                | NodeCardRejection::CardAbsent { .. }
        )
    }

    /// Whether the subject of the write is simply not there.
    pub const fn is_not_found(&self) -> bool {
        matches!(
            self,
            NodeCardRejection::UnknownRef { .. } | NodeCardRejection::NoBody { .. }
        )
    }
}

impl fmt::Display for NodeCardRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeCardRejection::UnknownRef { node_id, about } => write!(
                f,
                "`{node_id}` is not an entry of about `{about}`; a card summarizes one stored \
                 entry body inside its own about"
            ),
            NodeCardRejection::NoBody { node_id } => write!(
                f,
                "`{node_id}` has no stored body to summarize; there is nothing for a card to \
                 stand for"
            ),
            NodeCardRejection::SourceMoved {
                declared_revision,
                actual_revision,
                declared_record_digest,
                actual_record_digest,
            } => write!(
                f,
                "the body of this entry moved while the card was being written: declared \
                 revision {declared_revision} (`{declared_record_digest}`), stored revision \
                 {actual_revision} (`{actual_record_digest}`). Read the stored revision and \
                 author the card against it"
            ),
            NodeCardRejection::AuthoredAfterCut { authored_at } => write!(
                f,
                "this write stands at a historical cut and would stamp the card at \
                 {authored_at}, after it; author cards at the frontier"
            ),
            NodeCardRejection::CardTooLarge { card_bytes, limit } => write!(
                f,
                "a card is {card_bytes} bytes and the limit is {limit}; a compact view reads \
                 every card it shows, so an unbounded card is the cost it exists to remove"
            ),
            NodeCardRejection::CardAlreadyExists {
                actual_card_revision,
            } => write!(
                f,
                "a card already exists for this entry and language at card revision \
                 {actual_card_revision}; declare that revision in expect to replace it"
            ),
            NodeCardRejection::CardMoved {
                declared_card_revision,
                actual_card_revision,
            } => write!(
                f,
                "another reader replaced this card: declared card revision \
                 {declared_card_revision}, stored card revision {actual_card_revision}"
            ),
            NodeCardRejection::CardAbsent {
                declared_card_revision,
            } => write!(
                f,
                "no card is stored for this entry and language, but card revision \
                 {declared_card_revision} was declared; declare absent to author the first one"
            ),
            NodeCardRejection::EmptyCard => {
                f.write_str("a card needs text; an empty card compresses nothing")
            }
            NodeCardRejection::NotCompact {
                card_bytes,
                body_bytes,
            } => write!(
                f,
                "the card is {card_bytes} bytes and the body it would stand for is \
                 {body_bytes}; a card that is not shorter than its body cannot make a read \
                 cheaper"
            ),
        }
    }
}
