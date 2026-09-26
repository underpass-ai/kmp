use super::lexical_field::LexicalField;
use super::morphology::Morphology;
use super::question_contract::QuestionContract;
use super::question_contract_vocabulary::QuestionContractVocabulary;
use super::search_terms::informative_term_counts;
use super::shortlisted_partners::ShortlistedPartners;
use super::term_counts::TermCounts;

/// The facts of one about, read once so each new fact can be shortlisted
/// against them without sending the whole about to a judge (DESIGN L4 4e).
///
/// Anchors are read as the anchored gate reads a question
/// (`QuestionContract`): identifiers with a digit or `#`, quantities and
/// short bare numbers left soft, and an anchor most facts name read as a hub
/// and ignored. BM25 is the ranker's own, over this about's facts: a word
/// every fact says earns nothing here.
pub struct PartnerShortlist {
    morphology: Morphology,
    facts: Vec<(String, TermCounts)>,
    field: LexicalField,
}

impl PartnerShortlist {
    /// `facts` as `(ref, text)`, in the about's order.
    pub fn over<'a>(facts: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let facts = facts.into_iter().collect::<Vec<_>>();
        let morphology = Morphology::for_language(
            Morphology::read_language(facts.iter().map(|(_, text)| *text)).as_deref(),
        );
        let facts = facts
            .into_iter()
            .map(|(reference, text)| {
                (
                    reference.to_string(),
                    informative_term_counts(&with_aliases(text), &morphology),
                )
            })
            .collect::<Vec<_>>();
        let field = LexicalField::build(facts.iter().map(|(_, terms)| terms));
        Self {
            morphology,
            facts,
            field,
        }
    }

    /// The partners of the fact `reference` saying `text`: every other fact
    /// that names one of its hard anchors, then its `lexical` best BM25
    /// matches among the rest, the about's order breaking ties. An about of
    /// at most `lexical` other facts is shortlisted whole, as before any
    /// shortlist: a paraphrase that shares no word still reaches the judge.
    /// The fact itself is never its own partner.
    pub fn partners(&self, reference: &str, text: &str, lexical: usize) -> ShortlistedPartners {
        let vocabulary = QuestionContractVocabulary::shipped();
        let contract = QuestionContract::read(text, &self.morphology);
        let mut anchors = contract
            .anchors()
            .iter()
            .filter(|anchor| anchor.is_required())
            .map(|anchor| (anchor, self.named_by(reference, &anchor.term)))
            .filter(|(anchor, naming)| {
                !naming.is_empty()
                    && !vocabulary.is_hub(
                        self.field.document_frequency(&anchor.term),
                        self.field.documents(),
                    )
            })
            .collect::<Vec<_>>();
        let principal = anchors
            .iter()
            .enumerate()
            .min_by_key(|(position, (_, naming))| (naming.len(), *position))
            .map(|(position, _)| position);
        let (principal_anchor, sharing_principal) = match principal {
            Some(position) => {
                let (anchor, naming) = anchors.swap_remove(position);
                anchors.push((anchor, naming.clone()));
                (Some(anchor.written.clone()), naming)
            }
            None => (None, Vec::new()),
        };
        let anchored = self
            .facts
            .iter()
            .map(|(other, _)| other)
            .filter(|other| anchors.iter().any(|(_, naming)| naming.contains(*other)))
            .cloned()
            .collect::<Vec<_>>();
        let question = informative_term_counts(&with_aliases(text), &self.morphology)
            .terms()
            .map(|term| (term.clone(), 1.0))
            .collect();
        let mut scored = self
            .facts
            .iter()
            .enumerate()
            .filter(|(_, (other, _))| other != reference && !anchored.contains(other))
            .map(|(order, (other, terms))| {
                (self.field.score_weighted(&question, terms), order, other)
            })
            .collect::<Vec<_>>();
        scored.sort_by(|left, right| right.0.total_cmp(&left.0).then(left.1.cmp(&right.1)));
        ShortlistedPartners {
            anchored,
            lexical: scored
                .into_iter()
                .take(lexical)
                .map(|(_, _, other)| other.clone())
                .collect(),
            principal_anchor,
            sharing_principal,
        }
    }

    /// The other facts whose terms carry `term`, in the about's order.
    fn named_by(&self, reference: &str, term: &str) -> Vec<String> {
        self.facts
            .iter()
            .filter(|(other, terms)| other != reference && terms.count(term) > 0)
            .map(|(other, _)| other.clone())
            .collect()
    }
}

/// The text with the alias terms it spells (`INC-4711` also reads as
/// `inc4711`), as the anchored gate indexes a memory, so an anchor read off
/// one fact finds the others however they spelled it.
fn with_aliases(text: &str) -> String {
    let mut text = text.to_string();
    for term in QuestionContractVocabulary::shipped()
        .identifier_aliases()
        .text_terms(&text.clone())
    {
        text.push(' ');
        text.push_str(&term);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shortlist(facts: &[(&str, &str)]) -> PartnerShortlist {
        PartnerShortlist::over(facts.iter().copied())
    }

    #[test]
    fn a_shared_hard_anchor_shortlists_a_fact_that_shares_no_other_word() {
        let facts = [
            ("new", "INC-4711 rollback finished at noon."),
            ("a", "Customers saw errors during INC-4711."),
            ("b", "The rollback of the office printers finished."),
            ("c", "Lunch is at noon on Thursday."),
        ];
        let partners = shortlist(&facts).partners("new", facts[0].1, 0);
        assert_eq!(partners.anchored, vec!["a".to_string()]);
        assert!(partners.lexical.is_empty());
        assert_eq!(partners.principal_anchor.as_deref(), Some("INC-4711"));
        assert_eq!(partners.sharing_principal, vec!["a".to_string()]);
    }

    #[test]
    fn bm25_keeps_the_best_matches_and_never_the_fact_itself() {
        let facts = [
            ("new", "The Valkey cache lost its primary replica."),
            ("a", "Valkey replica failover took forty seconds."),
            ("b", "The cache warms up after a deploy."),
            ("c", "Invoices are generated as PDF."),
        ];
        let partners = shortlist(&facts).partners("new", facts[0].1, 1);
        assert!(partners.anchored.is_empty());
        assert_eq!(partners.lexical, vec!["a".to_string()]);
        let every = shortlist(&facts).partners("new", facts[0].1, 10);
        assert_eq!(
            every.lexical,
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
            "a small about is shortlisted whole, best first"
        );
        assert!(every.refs().all(|reference| reference != "new"));
        assert_eq!(every.principal_anchor, None);
    }

    #[test]
    fn the_principal_anchor_is_the_one_the_fewest_facts_name() {
        let facts = [
            ("new", "Release 2.5 fixes INC-9 for checkout."),
            ("a", "Release 2.5 is planned."),
            ("b", "Release 2.5 ships the new retry rule."),
            ("c", "INC-9 was reported by support."),
            ("d", "Unrelated note about lunch."),
            ("e", "Another unrelated note."),
        ];
        let partners = shortlist(&facts).partners("new", facts[0].1, 0);
        assert_eq!(partners.principal_anchor.as_deref(), Some("INC-9"));
        assert_eq!(partners.sharing_principal, vec!["c".to_string()]);
        assert_eq!(
            partners.anchored,
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }

    #[test]
    fn an_anchor_most_facts_name_is_a_hub_and_shortlists_nothing() {
        let mut facts = vec![("new", "Project P-77 kickoff notes.")];
        let others = (0..40)
            .map(|n| (format!("f{n}"), format!("P-77 status line number {n}.")))
            .collect::<Vec<_>>();
        facts.extend(others.iter().map(|(r, t)| (r.as_str(), t.as_str())));
        let partners = shortlist(&facts).partners("new", facts[0].1, 0);
        assert!(partners.anchored.is_empty(), "{partners:?}");
        assert_eq!(partners.principal_anchor, None);
    }
}
