use kmp_domain::RelationExplanation;

/// One kept relation incident to a candidate's refs, as the store holds it:
/// what [`super::lexical_reach::LexicalReach`] reads the words it can lend
/// from.
pub struct ReachRelation<'a> {
    pub source: &'a str,
    pub target: &'a str,
    pub relation_type: &'a str,
    pub explanation: &'a RelationExplanation,
}
