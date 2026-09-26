use std::collections::BTreeSet;

use kmp_domain::language::identifier_terms;

use super::anchor_strength::AnchorStrength;
use super::identifier_aliases::is_year;
use super::morphology::Morphology;
use super::question_anchor::QuestionAnchor;
use super::question_contract_vocabulary::QuestionContractVocabulary;
use super::question_form::QuestionForm;
use super::question_time::QuestionTime;
use super::question_vocabulary::QuestionVocabulary;
use super::search_terms::{CONTEXT_BOUNDARIES, fold_search_term, informative_tokens, search_key};

/// What one question asks, read off its words before any memory is read.
///
/// Pure and linear in the question. The anchored ask gate decides with it:
/// which identifiers must be found (hard anchors), which are only searched
/// (soft) and which are excluded (negated); the concepts that must stand
/// beside the principal anchor in one memory (the subject); the facets it
/// enumerates, which only ever break ties; whether half an answer is an
/// answer (the form); and what it asks of time.
#[derive(Debug, Clone, Default)]
pub(super) struct QuestionContract {
    anchors: Vec<QuestionAnchor>,
    /// Search key of each subject concept and the reader's word for it, in
    /// the order the question asks them.
    subject: Vec<(String, String)>,
    facets: BTreeSet<String>,
    facet_entry_kinds: BTreeSet<String>,
    form: QuestionForm,
    time: QuestionTime,
    /// See [`Self::asked`].
    asked: Option<String>,
    /// See [`Self::anchored_asked`].
    anchored_asked: Option<String>,
}

/// One whitespace token of the question as the contract reads it.
struct Word {
    /// Without edge punctuation, case kept.
    written: String,
    /// Folded, as the vocabularies list words.
    folded: String,
    /// The punctuation after it ends a clause.
    ends_clause: bool,
    /// It separates a list (`a, b`).
    ends_with_comma: bool,
}

impl Word {
    fn read(token: &str) -> Self {
        let keep = |character: char| character.is_alphanumeric() || matches!(character, '#' | '%');
        let start = token.find(keep).unwrap_or(token.len());
        let end = token.rfind(keep).map_or(start, |index| {
            index + token[index..].chars().next().map_or(0, char::len_utf8)
        });
        let written = token[start..end.max(start)].to_string();
        let trailing = &token[end.max(start)..];
        Self {
            folded: fold_search_term(&written),
            ends_clause: trailing.contains([',', ';', '.', '?', '!', ':']),
            ends_with_comma: trailing.contains(','),
            written,
        }
    }

    fn is_bare_number(&self) -> bool {
        !self.folded.is_empty() && self.folded.bytes().all(|byte| byte.is_ascii_digit())
    }

    /// A calendar date or a clock time: `2026-09-26`, `26/09`, `10:30`.
    fn is_date_or_time(&self) -> bool {
        let digits_only = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
        let parts = self.folded.split(['-', '/', ':', '.']).collect::<Vec<_>>();
        parts.len() >= 2
            && parts.iter().all(|part| digits_only(part))
            && (self.folded.contains(':')
                || parts
                    .first()
                    .is_some_and(|year| year.len() == 4 && is_year(year))
                || (self.folded.contains('/') && parts.iter().all(|part| part.len() <= 4)))
    }
}

/// How far after a bare number a unit may stand, through the words a range
/// is written with: `from 300 to 30 seconds`.
const UNIT_LOOKAHEAD: usize = 3;

impl QuestionContract {
    pub(super) fn read(question: &str, morphology: &Morphology) -> Self {
        let vocabulary = QuestionContractVocabulary::shipped();
        let families = QuestionVocabulary::shipped();
        let words = question
            .split_whitespace()
            .map(Word::read)
            .collect::<Vec<_>>();
        let folded = words
            .iter()
            .map(|word| word.folded.clone())
            .collect::<Vec<_>>();
        let negated = negated_words(&words, &folded, vocabulary);

        let mut anchors = Vec::<QuestionAnchor>::new();
        let mut aliased = Vec::<String>::new();
        for (index, word) in words.iter().enumerate() {
            for (term, strength) in anchor_terms(&words, index, vocabulary) {
                // `corte 10` names `c10`, the term `C10` reads as; `ADR-018`
                // names `adr18`, like `ADR 18`. The reader's words stay as
                // they were written.
                let (term, written) = match bound_alias(&words, index, strength, vocabulary) {
                    Some((alias, written)) => {
                        if alias != term {
                            aliased.push(alias.clone());
                        }
                        (alias, written)
                    }
                    None => (term, word.written.clone()),
                };
                let anchor = QuestionAnchor {
                    written,
                    term,
                    strength,
                    negated: negated[index],
                };
                match anchors.iter_mut().find(|known| known.term == anchor.term) {
                    // Named twice, the stronger reading stands: required
                    // once is required.
                    Some(known) if anchor.is_required() && !known.is_required() => *known = anchor,
                    Some(_) => {}
                    None => anchors.push(anchor),
                }
            }
        }

        // What the question asks stands before its first context word; a
        // question that opens with one (`When did ...`) asks with all of it.
        let main_end = folded
            .iter()
            .position(|word| CONTEXT_BOUNDARIES.contains(&word.as_str()))
            .filter(|end| *end > 0)
            .unwrap_or(words.len());
        let main = 0..main_end;

        let mut subject = Vec::<(String, String)>::new();
        let mut facets = BTreeSet::new();
        let mut facet_entry_kinds = BTreeSet::new();
        let mut coordinations = 0;
        let mut facet_words = vec![false; words.len()];
        let mut asks_existence = false;
        for index in main {
            let word = &words[index];
            if negated[index] {
                continue;
            }
            if word.ends_with_comma {
                coordinations += 1;
            }
            if vocabulary.is_coordination(&word.folded) {
                coordinations += 1;
                continue;
            }
            asks_existence |= vocabulary.asks_existence(&word.folded);
            if let Some((facet, kinds)) = families.facet_of(&word.folded) {
                facet_words[index] = true;
                facets.insert(facet.to_string());
                facet_entry_kinds.extend(kinds.iter().cloned());
                continue;
            }
            if !identifier_terms(&word.written).is_empty()
                || vocabulary.carries_no_subject(&word.folded)
            {
                continue;
            }
            let parts = informative_tokens(&word.written).collect::<Vec<_>>();
            let whole = parts.len() == 1;
            for part in parts {
                let key = search_key(&part, morphology);
                if !vocabulary.carries_no_subject(&part)
                    && !subject.iter().any(|(known, _)| *known == key)
                {
                    // What `missing` reports is the reader's word as written,
                    // accents and all; a word read as several parts reports
                    // each part.
                    let reader = if whole { word.written.clone() } else { part };
                    subject.push((key, reader));
                }
            }
        }
        let form = if facets.len() >= 2 || coordinations >= 2 || asks_existence {
            QuestionForm::Enumerative
        } else {
            QuestionForm::Singular
        };
        let time = if folded
            .iter()
            .any(|word| families.family_of(word) == Some("lifecycle"))
        {
            QuestionTime::History
        } else if folded.iter().any(|word| vocabulary.asks_state(word)) {
            QuestionTime::State
        } else {
            QuestionTime::Unstated
        };
        // The question the ranker reads under the gate: without what it
        // excluded, which it does not ask for, and with the alias terms its
        // anchors name, which the memories are read with too.
        let read_as = |skip: &dyn Fn(usize) -> bool| {
            question
                .split_whitespace()
                .enumerate()
                .filter(|(index, _)| !skip(*index))
                .map(|(_, token)| token)
                .chain(aliased.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let asked = (negated.iter().any(|negated| *negated) || !aliased.is_empty())
            .then(|| read_as(&|index| negated[index]));
        let anchored_asked = (asked.is_some() || facet_words.iter().any(|facet| *facet))
            .then(|| read_as(&|index| negated[index] || facet_words[index]));
        Self {
            anchors,
            subject,
            facets,
            facet_entry_kinds,
            form,
            time,
            asked,
            anchored_asked,
        }
    }

    /// The question as the ranker reads it under the gate, when that is not
    /// the question itself: its negated stretches left out and its alias
    /// terms added.
    pub(super) fn asked(&self) -> Option<&str> {
        self.asked.as_deref()
    }

    /// The question as the ranker reads it when an anchor decides: as
    /// [`Self::asked`], and without the facets it enumerates, which only
    /// break ties by entry kind and must not raise the bar a memory that
    /// names the anchor has to clear (`What was decided for C4?` asks about
    /// C4).
    pub(super) fn anchored_asked(&self) -> Option<&str> {
        self.anchored_asked.as_deref()
    }

    /// The terms of every anchor the question requires or searches for, not
    /// the ones it excludes.
    pub(super) fn unnegated_terms(&self) -> BTreeSet<String> {
        self.anchors
            .iter()
            .filter(|anchor| !anchor.negated)
            .map(|anchor| anchor.term.clone())
            .collect()
    }

    /// Every anchor the question named, in the order it named them.
    pub(super) fn anchors(&self) -> &[QuestionAnchor] {
        &self.anchors
    }

    /// Whether any anchor must be found for the question to be answered.
    pub(super) fn requires_anchors(&self) -> bool {
        self.anchors.iter().any(QuestionAnchor::is_required)
    }

    /// The terms of the anchors the question excludes.
    pub(super) fn negated_terms(&self) -> BTreeSet<String> {
        self.anchors
            .iter()
            .filter(|anchor| anchor.negated)
            .map(|anchor| anchor.term.clone())
            .collect()
    }

    /// Search key of each concept that must stand beside the principal
    /// anchor, with the reader's word for it.
    pub(super) fn subject(&self) -> &[(String, String)] {
        &self.subject
    }

    /// The facets the question enumerates. They shape nothing the gate
    /// requires; the tie-break reads their entry kinds.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn facets(&self) -> &BTreeSet<String> {
        &self.facets
    }

    /// The entry kinds that state the facets the question enumerates.
    pub(super) fn facet_entry_kinds(&self) -> &BTreeSet<String> {
        &self.facet_entry_kinds
    }

    pub(super) fn form(&self) -> QuestionForm {
        self.form
    }

    /// What the question's words ask of time. The gate does not decide on
    /// it: `as_of` and `interval` select as they always did, and the
    /// lifecycle rescue that will read it is a later step (P7).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn time(&self) -> QuestionTime {
        self.time
    }
}

/// Which words stand in a negated stretch: from a negation to the end of
/// its clause. The negation itself is part of it.
fn negated_words(
    words: &[Word],
    folded: &[String],
    vocabulary: &QuestionContractVocabulary,
) -> Vec<bool> {
    let mut negated = vec![false; words.len()];
    let mut open = false;
    let mut index = 0;
    while index < words.len() {
        if vocabulary.breaks_clause(&folded[index]) {
            open = false;
        } else if let Some(span) = vocabulary.negation_at(folded, index) {
            open = true;
            for covered in negated.iter_mut().skip(index).take(span - 1) {
                *covered = true;
            }
            index += span - 1;
        }
        negated[index] = open;
        if words[index].ends_clause {
            open = false;
        }
        index += 1;
    }
    negated
}

/// The anchor terms the word at `index` names, each with its strength.
fn anchor_terms(
    words: &[Word],
    index: usize,
    vocabulary: &QuestionContractVocabulary,
) -> Vec<(String, AnchorStrength)> {
    let word = &words[index];
    let terms = identifier_terms(&word.written);
    if terms.len() > 1 {
        // A list names each identifier; a range names two ends and asks for
        // what lies between them, which neither end states alone.
        let listed = word.written.contains(['+', ',', '/']);
        let strength = if listed {
            AnchorStrength::Hard
        } else {
            AnchorStrength::Soft
        };
        return terms.into_iter().map(|term| (term, strength)).collect();
    }
    terms
        .into_iter()
        .map(|term| (term, single_strength(words, index, vocabulary)))
        .collect()
}

/// The alias term an anchor names and the reader's words for it: a hard
/// number a guide word introduces (`corte 10`, written `corte 10`), or a
/// token that writes a kind and its number together (`ADR-018`).
fn bound_alias(
    words: &[Word],
    index: usize,
    strength: AnchorStrength,
    vocabulary: &QuestionContractVocabulary,
) -> Option<(String, String)> {
    let aliases = vocabulary.identifier_aliases();
    let word = &words[index];
    if strength == AnchorStrength::Hard
        && let Some(previous) = index.checked_sub(1).map(|previous| &words[previous])
        && let Some(term) = aliases.bound(&previous.folded, &word.folded)
    {
        return Some((term, format!("{} {}", previous.written, word.written)));
    }
    aliases
        .joined(&word.folded)
        .map(|term| (term, word.written.clone()))
}

/// Whether one identifier the question names is required.
///
/// Soft: a quantity (`30s`, `80%`, or a bare number a unit follows), a date
/// or a year, and a bare number under three digits that no word such as
/// `issue` or `corte` introduces. Everything else that carries a digit or a
/// `#` is hard.
fn single_strength(
    words: &[Word],
    index: usize,
    vocabulary: &QuestionContractVocabulary,
) -> AnchorStrength {
    let word = &words[index];
    if vocabulary.is_quantity(&word.folded) || word.is_date_or_time() {
        return AnchorStrength::Soft;
    }
    if !word.is_bare_number() {
        return AnchorStrength::Hard;
    }
    let unit_follows = words
        .iter()
        .skip(index + 1)
        .take(UNIT_LOOKAHEAD)
        .take_while(|next| {
            vocabulary.is_unit(&next.folded)
                || vocabulary.connects_range(&next.folded)
                || next.is_bare_number()
        })
        .any(|next| vocabulary.is_unit(&next.folded));
    let introduced = index
        .checked_sub(1)
        .is_some_and(|previous| vocabulary.is_guide_word(&words[previous].folded));
    if unit_follows || is_year(&word.folded) || (word.folded.len() < 3 && !introduced) {
        AnchorStrength::Soft
    } else {
        AnchorStrength::Hard
    }
}
