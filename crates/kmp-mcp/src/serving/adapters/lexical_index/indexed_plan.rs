use std::collections::{BTreeMap, BTreeSet};

use kmp_proto_mapping::v1beta1::{
    IndexedAsk, IndexedFieldStats, IndexedQuestion, LexicalBridge, LexicalRow,
};

use super::about_stats::AboutStats;
use super::sqlite_lexical_sidecar::SqliteLexicalSidecar;
use crate::serving::ports::lexical_candidates::LexicalCandidates;

/// The largest share of the about, in twentieths, an ask answered from the
/// postings may read. Reading a candidate point by point costs about twice
/// what reading it with the whole about does (measured: 0.5 ms against 0.27
/// at 10^4 entries, 0.65 against 0.28 at 10^5); on the frozen real store
/// (1,042 asks) the index answers faster below 40 % of the about and slower
/// above it, so past 35 % the ask reads the about. P14 (top-k) bounds what
/// an answer reads below that.
const MAX_SHARE_TWENTIETHS: u64 = 7;

/// Why `count` candidates of an about of `documents` are too many for the
/// index to save anything over reading the about, if they are.
pub(super) fn too_many(count: usize, documents: u64) -> Option<Declined> {
    (count as u64 * 20 > documents * MAX_SHARE_TWENTIETHS)
        .then_some("the candidates cover too much of the about")
}

/// What an ask answered from the lexical index reads (DESIGN L6, P13): the
/// candidates the postings of its words and of their associations reach,
/// and the whole about's statistics and lifecycle to rank them against.
#[derive(Debug, Clone)]
pub(super) struct IndexedPlan {
    pub(super) candidates: BTreeSet<String>,
    /// How many candidates the whole about holds.
    pub(super) documents: u64,
    pub(super) indexed: IndexedAsk,
}

/// Why an ask is not answered from the index; it then reads the about.
pub(super) type Declined = &'static str;

impl IndexedPlan {
    /// The plan for `question` over `about`, or why the index does not
    /// answer it. `bounded` declines a question whose candidates are too
    /// many for the index to save anything ([`too_many`]); unbounded (the
    /// `verify` mode) every ask the index can hold is planned, to measure it.
    pub(super) fn read(
        sidecar: &SqliteLexicalSidecar,
        about: &str,
        question: &str,
        bridge: &LexicalBridge,
        bounded: bool,
    ) -> Result<Result<Self, Declined>, String> {
        let Some(stats) = sidecar.stats(about)? else {
            return Ok(Err("about not built"));
        };
        // A candidate its judged expansions alone reach is rescued from a
        // field measured over every expanded candidate of the about.
        if stats.expanded > 0 {
            return Ok(Err("the about holds search expansions"));
        }
        let question = IndexedQuestion::read(question, stats.language.as_deref());
        let mut seeds = question.terms();
        // With a table installed, the question also reaches the words it
        // bridges to in the whole about's vocabulary (three per word at most).
        let vocabulary = if bridge.is_silent() {
            None
        } else {
            let vocabulary = sidecar.vocabulary(about)?;
            seeds.extend(question.bridged(bridge, &vocabulary));
            Some(std::sync::Arc::new(vocabulary))
        };
        let first = candidates(sidecar, about, &seeds)?;
        if let Some(why) = too_many(first.len(), stats.documents).filter(|_| bounded) {
            return Ok(Err(why));
        }
        let mut probed = seeds.clone();
        probed.extend(held_terms(first.values()));
        // Each term's df is read once, under both readings.
        let mut frequencies = sidecar.document_frequencies(about, &probed)?;
        let rows = first.values().cloned().collect::<Vec<_>>();
        let mut associated = BTreeSet::new();
        for aliased in [false, true] {
            let field = field_stats(&stats, &probed, &frequencies, aliased);
            associated.extend(question.associations(&field, aliased, &rows));
        }
        let extra = associated
            .iter()
            .filter(|term| !seeds.contains(*term))
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut reached = first;
        if !extra.is_empty() {
            reached.extend(candidates(sidecar, about, &extra)?);
        }
        if let Some(why) = too_many(reached.len(), stats.documents).filter(|_| bounded) {
            return Ok(Err(why));
        }
        let mut terms = seeds;
        terms.extend(associated);
        terms.extend(held_terms(reached.values()));
        let unread = terms
            .iter()
            .filter(|term| !frequencies.contains_key(*term))
            .cloned()
            .collect::<BTreeSet<_>>();
        frequencies.extend(sidecar.document_frequencies(about, &unread)?);
        let indexed = IndexedAsk {
            language: stats.language.clone(),
            plain: field_stats(&stats, &terms, &frequencies, false),
            aliased: field_stats(&stats, &terms, &frequencies, true),
            lifecycle: sidecar.lifecycle(about)?,
            vocabulary,
        };
        Ok(Ok(Self {
            candidates: reached.into_keys().collect(),
            documents: stats.documents,
            indexed,
        }))
    }
}

fn candidates(
    sidecar: &SqliteLexicalSidecar,
    about: &str,
    terms: &BTreeSet<String>,
) -> Result<BTreeMap<String, LexicalRow>, String> {
    LexicalCandidates::candidates(sidecar, about, &terms.iter().cloned().collect::<Vec<_>>())
}

/// Every term a row carries under either reading.
fn held_terms<'r>(rows: impl Iterator<Item = &'r LexicalRow>) -> BTreeSet<String> {
    rows.flat_map(|row| {
        row.terms()
            .iter()
            .filter(|term| term.is_held())
            .map(|term| term.term.clone())
    })
    .collect()
}

/// One reading of the about's collection over `terms`, from their df
/// under both readings (`[content, direct, aliased content, aliased direct]`).
fn field_stats(
    stats: &AboutStats,
    terms: &BTreeSet<String>,
    frequencies: &BTreeMap<String, [u64; 4]>,
    aliased: bool,
) -> IndexedFieldStats {
    let mut field = IndexedFieldStats {
        documents: stats.documents,
        content_length: stats.content_length(aliased),
        direct_length: stats.direct_length(aliased),
        ..IndexedFieldStats::default()
    };
    let slot = if aliased { 2 } else { 0 };
    for term in terms {
        let Some(frequency) = frequencies.get(term) else {
            continue;
        };
        if frequency[slot] > 0 {
            field.content_df.insert(term.clone(), frequency[slot]);
        }
        if frequency[slot + 1] > 0 {
            field.direct_df.insert(term.clone(), frequency[slot + 1]);
        }
    }
    field
}
