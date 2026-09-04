use super::shared_label::SharedLabel;
use std::collections::BTreeMap;

/// The most proposals one fact takes part in. A common key would otherwise
/// join everything with everything; three per fact keeps a proposal worth
/// reading.
pub const MAX_PROPOSALS_PER_FACT: usize = 3;

/// What made two facts of different abouts look like the same thing. Each
/// signal carries what it read, so a proposal is reproducible bit for bit
/// and says why.
#[derive(Debug, Clone, PartialEq)]
pub enum ProposalSignal {
    /// Both texts carry an identifier that is rare across the span.
    Identifier { shared: Vec<String>, idf: f64 },
    /// Both English summaries share most of their concepts, some of them
    /// through the lexical-bridge table.
    Summary {
        shared_terms: Vec<String>,
        bridged: Vec<String>,
        share: f64,
    },
    /// Both texts name the same proper name.
    Entity { entities: Vec<String> },
}

impl ProposalSignal {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Identifier { .. } => "identifier",
            Self::Summary { .. } => "summary",
            Self::Entity { .. } => "entity",
        }
    }

    /// The signal's weight: an identifier rare in the span says the most, a
    /// summary that matches says a little less, a name alone the least.
    pub fn weight(&self) -> u32 {
        match self {
            Self::Identifier { .. } => 4,
            Self::Summary { .. } => 3,
            Self::Entity { .. } => 2,
        }
    }
}

/// Two facts of different abouts the kernel proposes are about the same
/// thing. Never stored: a writer may declare it, with this as the proof.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposedLink {
    from: String,
    to: String,
    signals: Vec<ProposalSignal>,
    /// The label both stand in, when they share one; sharing one weighs.
    /// The dimension kind is its key and the scope its value.
    dimension: String,
    scope_id: String,
    weight: u32,
}

impl ProposedLink {
    /// Sharing a scope adds this to the signals' own weight.
    pub const SHARED_SCOPE_WEIGHT: u32 = 2;

    pub fn new(
        from: impl Into<String>,
        to: impl Into<String>,
        mut signals: Vec<ProposalSignal>,
        label: Option<SharedLabel>,
    ) -> Self {
        signals.sort_by_key(|signal| std::cmp::Reverse(signal.weight()));
        let (dimension, scope_id) = label
            .map(|label| (label.key().to_string(), label.value().to_string()))
            .unwrap_or_default();
        let weight = signals.iter().map(ProposalSignal::weight).sum::<u32>()
            + if scope_id.is_empty() {
                0
            } else {
                Self::SHARED_SCOPE_WEIGHT
            };
        Self {
            from: from.into(),
            to: to.into(),
            signals,
            dimension,
            scope_id,
            weight,
        }
    }

    pub fn from(&self) -> &str {
        &self.from
    }

    pub fn to(&self) -> &str {
        &self.to
    }

    pub fn signals(&self) -> &[ProposalSignal] {
        &self.signals
    }

    pub fn dimension(&self) -> &str {
        &self.dimension
    }

    pub fn scope_id(&self) -> &str {
        &self.scope_id
    }

    pub fn weight(&self) -> u32 {
        self.weight
    }

    /// The sentence a reader gets: every signal, in the words it read.
    pub fn why(&self) -> String {
        let mut parts = self
            .signals
            .iter()
            .map(|signal| match signal {
                ProposalSignal::Identifier { shared, idf } => format!(
                    "both carry `{}`, rare across the span (idf {idf:.2})",
                    shared.join("`, `")
                ),
                ProposalSignal::Summary {
                    shared_terms,
                    bridged,
                    share,
                } => {
                    let mut sentence = format!(
                        "their summaries share {:.0}% of their concepts (`{}`)",
                        share * 100.0,
                        shared_terms.join("`, `")
                    );
                    if !bridged.is_empty() {
                        sentence.push_str(&format!(
                            ", {} through the lexical-bridge table",
                            bridged.join(", ")
                        ));
                    }
                    sentence
                }
                ProposalSignal::Entity { entities } => {
                    format!("both name `{}`", entities.join("`, `"))
                }
            })
            .collect::<Vec<_>>();
        if !self.scope_id.is_empty() {
            parts.push(format!(
                "both stand in `{}={}`",
                self.dimension, self.scope_id
            ));
        }
        let mut why = parts.join("; ");
        if let Some(first) = why.get(..1) {
            why = first.to_uppercase() + &why[1..];
        }
        why.push('.');
        why
    }
}

/// Keeps at most [`MAX_PROPOSALS_PER_FACT`] proposals per fact, the
/// strongest first, so a common key never joins everything with
/// everything. The order is deterministic: weight down, then the pair.
pub fn cap_proposals_per_fact(mut proposals: Vec<ProposedLink>) -> Vec<ProposedLink> {
    proposals.sort_by(|left, right| {
        right
            .weight
            .cmp(&left.weight)
            .then_with(|| (left.from(), left.to()).cmp(&(right.from(), right.to())))
    });
    let mut taken = BTreeMap::<String, usize>::new();
    proposals
        .into_iter()
        .filter(|proposal| {
            let from = taken.get(proposal.from()).copied().unwrap_or_default();
            let to = taken.get(proposal.to()).copied().unwrap_or_default();
            if from >= MAX_PROPOSALS_PER_FACT || to >= MAX_PROPOSALS_PER_FACT {
                return false;
            }
            *taken.entry(proposal.from().to_string()).or_default() += 1;
            *taken.entry(proposal.to().to_string()).or_default() += 1;
            true
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(
        from: &str,
        to: &str,
        weight_signals: Vec<ProposalSignal>,
        label: Option<(&str, &str)>,
    ) -> ProposedLink {
        ProposedLink::new(
            from,
            to,
            weight_signals,
            label.map(|(key, value)| SharedLabel::new(key, value)),
        )
    }

    fn identifier(shared: &str, idf: f64) -> ProposalSignal {
        ProposalSignal::Identifier {
            shared: vec![shared.to_string()],
            idf,
        }
    }

    #[test]
    fn the_weight_sums_the_signals_and_the_shared_scope_and_the_why_names_them() {
        let proposal = link(
            "a:e1",
            "b:e1",
            vec![
                ProposalSignal::Entity {
                    entities: vec!["Valkey".to_string()],
                },
                identifier("#469", 1.1),
            ],
            Some(("release", "spring")),
        );
        assert_eq!(proposal.weight(), 4 + 2 + 2);
        assert_eq!(
            proposal.signals()[0].name(),
            "identifier",
            "strongest first"
        );
        let why = proposal.why();
        assert!(why.starts_with("Both carry `#469`"), "{why}");
        assert!(why.contains("both name `Valkey`"), "{why}");
        assert!(why.ends_with("; both stand in `release=spring`."), "{why}");
    }

    #[test]
    fn the_cap_keeps_the_strongest_three_per_fact() {
        let mut proposals = Vec::new();
        for index in 0..6 {
            proposals.push(link(
                "a:hub",
                &format!("b:e{index}"),
                vec![identifier("#469", 1.0 + f64::from(index))],
                None,
            ));
        }
        proposals.push(link("a:other", "b:e0", vec![identifier("#470", 3.0)], None));
        let kept = cap_proposals_per_fact(proposals);
        let hub = kept
            .iter()
            .filter(|proposal| proposal.from() == "a:hub")
            .count();
        assert_eq!(hub, MAX_PROPOSALS_PER_FACT);
        assert!(
            kept.iter()
                .any(|proposal| proposal.from() == "a:other" && proposal.to() == "b:e0"),
            "a fact with room keeps its own proposal"
        );
    }
}
