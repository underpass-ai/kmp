use std::collections::{BTreeMap, BTreeSet};

use kmp_proto_mapping::v1beta1::{
    FloorBound, IndexedAsk, IndexedFieldStats, IndexedQuestion, LexicalBridge, LexicalRow,
};

use super::about_stats::AboutStats;
use super::index_limits::IndexLimits;
use super::sqlite_lexical_sidecar::SqliteLexicalSidecar;
use crate::serving::ports::lexical_candidates::LexicalCandidates;

/// What an ask answered from the lexical index reads (DESIGN L6, P13): the
/// candidates the postings of its words and of their associations reach,
/// less those MaxScore proves cannot clear the floor (P14, [`FloorBound`]),
/// and the whole about's statistics and lifecycle to rank them against.
#[derive(Debug, Clone)]
pub(super) struct IndexedPlan {
    pub(super) candidates: BTreeSet<String>,
    /// How many candidates the postings reached before the floor bound.
    pub(super) reached: usize,
    /// How many candidates the whole about holds.
    pub(super) documents: u64,
    pub(super) indexed: IndexedAsk,
}

impl IndexedPlan {
    /// The plan for `question` over `about`, or why the index does not
    /// answer it. `bounded` declines a question whose candidates are too
    /// many for the index to save anything ([`IndexLimits::too_many`]);
    /// unbounded (the `verify` mode) every ask the index can hold is planned,
    /// to measure it. `prune` leaves unread the candidates the floor bound
    /// refuses (P14, on unless `KMP_LEXICAL_MAXSCORE=off`); the bound applies
    /// to what is left to read.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn read(
        sidecar: &SqliteLexicalSidecar,
        about: &str,
        question: &str,
        bridge: &LexicalBridge,
        bounded: Option<IndexLimits>,
        deeper: bool,
        prune: bool,
    ) -> Result<Result<Self, &'static str>, String> {
        let Some(stats) = sidecar.stats(about)? else {
            return Ok(Err("about not built"));
        };
        // A deeper read is the indexed one while nothing lies past it.
        if deeper && stats.far > 0 {
            return Ok(Err("nodes lie past the indexed depth"));
        }
        // A candidate its judged expansions alone reach is rescued from a
        // field measured over every expanded candidate of the about.
        if stats.expanded > 0 {
            return Ok(Err("the about holds search expansions"));
        }
        let question = IndexedQuestion::read(question, stats.language.as_deref());
        let mut seeds = question.terms();
        // With a table installed, the question also reaches the words it
        // bridges to in the whole about's vocabulary (three per word at most).
        let mut bridged = BTreeSet::new();
        let vocabulary = if bridge.is_silent() {
            None
        } else {
            let vocabulary = sidecar.vocabulary(about)?;
            bridged = question.bridged(bridge, &vocabulary);
            seeds.extend(bridged.iter().cloned());
            Some(std::sync::Arc::new(vocabulary))
        };
        let first = candidates(sidecar, about, &seeds)?;
        // Unpruned, the candidates are what the ask reads, and a question
        // that reaches too many of them is sent to the about before its
        // associations are counted.
        if !prune
            && let Some(why) =
                bounded.and_then(|limits| limits.too_many(first.len(), stats.documents))
        {
            return Ok(Err(why));
        }
        let seed_rows = std::sync::Arc::new(first.values().cloned().collect::<Vec<_>>());
        let mut probed = seeds.clone();
        probed.extend(held_terms(first.values()));
        // Each term's df is read once, under both readings.
        let mut frequencies = sidecar.document_frequencies(about, &probed)?;
        let mut associated = BTreeSet::new();
        for aliased in [false, true] {
            let field = field_stats(&stats, &probed, &frequencies, aliased);
            associated.extend(question.associations(&field, aliased, &seed_rows));
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
        let mut terms = seeds;
        terms.extend(associated.iter().cloned());
        terms.extend(held_terms(reached.values()));
        let unread = terms
            .iter()
            .filter(|term| !frequencies.contains_key(*term))
            .cloned()
            .collect::<BTreeSet<_>>();
        frequencies.extend(sidecar.document_frequencies(about, &unread)?);
        let plain = field_stats(&stats, &terms, &frequencies, false);
        let aliased = field_stats(&stats, &terms, &frequencies, true);
        // MaxScore against the floor (P14): a candidate no reading of the
        // question lets clear it, and that carries no association or bridged
        // word, is ranked by nobody and rescued by nobody; it is left unread.
        let bound = prune
            .then(|| {
                // The associations of the question's words include the
                // words themselves, which the bound weighs; what else they
                // bring, and every bridged word, keeps a candidate read.
                let asked = question.terms();
                let mut kept = associated
                    .into_iter()
                    .filter(|term| !asked.contains(term))
                    .collect::<BTreeSet<_>>();
                kept.extend(bridged);
                FloorBound::read(
                    &question,
                    &plain,
                    &aliased,
                    bridge,
                    vocabulary.as_deref().map(Vec::as_slice),
                    kept,
                    reached.values(),
                )
            })
            .flatten();
        let count = reached.len();
        let read = reached
            .into_iter()
            .filter(|(_, row)| !bound.as_ref().is_some_and(|bound| bound.prunes(row)))
            .map(|(candidate, _)| candidate)
            .collect::<BTreeSet<_>>();
        if let Some(why) = bounded.and_then(|limits| limits.too_many(read.len(), stats.documents)) {
            return Ok(Err(why));
        }
        let indexed = IndexedAsk {
            language: stats.language.clone(),
            plain,
            aliased,
            lifecycle: sidecar.lifecycle(about)?,
            vocabulary,
            seed_rows: Some(seed_rows),
        };
        Ok(Ok(Self {
            candidates: read,
            reached: count,
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
