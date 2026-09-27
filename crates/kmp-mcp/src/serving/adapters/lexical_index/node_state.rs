use kmp_proto_mapping::v1beta1::LanguageSignals;

/// What the lexical sidecar keeps about one node of an about's neighbourhood
/// (every node an ask of the about reaches in two hops), enough to follow
/// the ask's own selection one write at a time:
///
/// - `hop`: 0 for the about itself, 1 or 2 for the hops it is reached in;
/// - `selected_in` and `selected_out`: how many `contains_entry` edges the
///   selection keeps into and out of it (a selected entry, a label);
/// - `supports_selected`: how many `supports` edges leave it for a selected
///   entry, which is what brings a piece of evidence into the selection;
/// - what it adds to the about's language while selected.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct NodeState {
    pub(super) hop: u8,
    pub(super) evidence_kind: bool,
    pub(super) selected_in: u64,
    pub(super) selected_out: u64,
    pub(super) supports_selected: u64,
    pub(super) signals: LanguageSignals,
    pub(super) carries_summary: bool,
}

const STATE_LAYOUT: u8 = 1;

impl NodeState {
    /// Whether the ask's selection keeps the node: the about, an entry a
    /// kept `contains_entry` reaches, a label that keeps one, or evidence
    /// supporting a kept entry.
    pub(super) fn included(&self) -> bool {
        self.hop == 0
            || self.selected_in > 0
            || self.selected_out > 0
            || (self.evidence_kind && self.supports_selected > 0)
    }

    /// Whether a selected `contains_entry` reaches it, which is what makes
    /// its text a candidate.
    pub(super) fn selected_entry(&self) -> bool {
        self.selected_in > 0
    }

    pub(super) fn encode(&self) -> Vec<u8> {
        let signals = self.signals.encode();
        let mut buffer = Vec::with_capacity(16 + signals.len());
        buffer.push(STATE_LAYOUT);
        buffer.push(self.hop);
        buffer.push(u8::from(self.evidence_kind) | (u8::from(self.carries_summary) << 1));
        for count in [self.selected_in, self.selected_out, self.supports_selected] {
            buffer.extend_from_slice(&count.to_le_bytes());
        }
        buffer.extend_from_slice(&signals);
        buffer
    }

    pub(super) fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 27 || bytes[0] != STATE_LAYOUT {
            return Err("lexical node state has an unknown layout".into());
        }
        let count = |at: usize| {
            let mut raw = [0u8; 8];
            raw.copy_from_slice(&bytes[at..at + 8]);
            u64::from_le_bytes(raw)
        };
        Ok(Self {
            hop: bytes[1],
            evidence_kind: bytes[2] & 1 == 1,
            carries_summary: bytes[2] & 2 == 2,
            selected_in: count(3),
            selected_out: count(11),
            supports_selected: count(19),
            signals: LanguageSignals::decode(&bytes[27..])?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_round_trips_and_reads_its_selection() {
        let state = NodeState {
            hop: 1,
            evidence_kind: true,
            selected_in: 0,
            selected_out: 0,
            supports_selected: 2,
            signals: LanguageSignals::of_texts(["the valve froze and the crew replaced it"]),
            carries_summary: true,
        };
        assert_eq!(NodeState::decode(&state.encode()).expect("fixture"), state);
        assert!(state.included());
        assert!(!state.selected_entry());
        let unsupported = NodeState {
            supports_selected: 0,
            ..state
        };
        assert!(!unsupported.included());
        assert!(NodeState::decode(&[9, 1]).is_err());
    }
}
