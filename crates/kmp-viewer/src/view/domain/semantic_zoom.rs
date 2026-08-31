//! The rungs of the semantic-zoom ladder an intent may ask for.

/// The zoom changes representation, not just size. `evidence` is famously not
/// a rung — it is a selection state — and the closed vocabulary here is what
/// refuses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticZoom {
    /// Density ribbons — the shape of the memory, no nodes.
    Atlas,
    /// Entries and tight clusters, braided.
    Episode,
    /// Individual entries with their relations.
    Moment,
}

impl SemanticZoom {
    /// The advertised names, in ladder order.
    pub const NAMES: [&'static str; 3] = ["atlas", "episode", "moment"];

    /// Parses an advertised rung; anything else is not on the ladder.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "atlas" => Some(SemanticZoom::Atlas),
            "episode" => Some(SemanticZoom::Episode),
            "moment" => Some(SemanticZoom::Moment),
            _ => None,
        }
    }

    /// The advertised name of this rung.
    pub fn as_str(self) -> &'static str {
        match self {
            SemanticZoom::Atlas => "atlas",
            SemanticZoom::Episode => "episode",
            SemanticZoom::Moment => "moment",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_rung_parses_back_to_itself_and_evidence_is_not_one() {
        for name in SemanticZoom::NAMES {
            let rung = SemanticZoom::parse(name).expect("an advertised rung parses");
            assert_eq!(rung.as_str(), name);
        }
        assert_eq!(SemanticZoom::parse("evidence"), None);
    }
}
