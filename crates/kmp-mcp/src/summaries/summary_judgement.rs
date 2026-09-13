//! One concept: where one memory stands with respect to its English search
//! summary, and why.
//!
//! Two readings, kept apart because they answer to different authorities.
//! The state is the writer's rule, and the faults that come with it are the
//! kernel's lint, spoken in its own words — the same sentences `kmp_ingest`
//! and `kmp_write_memory` put in front of a writer. The weaknesses above
//! that floor are this module's own, and are read against the about around
//! the memory rather than against the memory alone.
//!
//! Nothing here touches JSON: it judges the histories `bundle_entries` has
//! already decoded.

use kmp_domain::SearchSummary;
use kmp_domain::language::{KERNEL_LANGUAGE, LanguageVocabulary, informative_tokens};

use super::about_lexicon::AboutLexicon;
use super::audited_summary::AuditedSummary;
use super::entry_history::EntryHistory;
use super::summary_state::SummaryState;
use super::summary_weakness::SummaryWeakness;

/// Where one memory stands, and why.
pub(crate) fn judge(history: &EntryHistory, lexicon: &AboutLexicon) -> AuditedSummary {
    let latest = history.latest();
    let (state, faults, weaknesses) = match latest.summary.as_deref() {
        None => (
            missing_or_not_required(&latest.text),
            Vec::new(),
            Vec::new(),
        ),
        Some(summary) => match SearchSummary::lint(&latest.text, summary) {
            Err(faults) => (SummaryState::Refused, faults, Vec::new()),
            Ok(_) => (
                SummaryState::Stands,
                Vec::new(),
                weaknesses(history, lexicon, summary),
            ),
        },
    };
    let needs_a_writer = state.owes_a_summary() || !weaknesses.is_empty();
    AuditedSummary {
        about: history.about.clone(),
        reference: history.reference.clone(),
        kind: latest.kind.clone(),
        state,
        text: needs_a_writer.then(|| latest.text.clone()),
        summary: latest.summary.clone(),
        summary_by: latest.summary_by.clone(),
        faults,
        weaknesses,
    }
}

/// The writer's rule for a memory with no summary at all: it owes one only
/// when an English question cannot reach its text as it stands.
fn missing_or_not_required(text: &str) -> SummaryState {
    match LanguageVocabulary::shipped().leans_in(text) {
        Some(language) if language != KERNEL_LANGUAGE => SummaryState::Missing,
        _ => SummaryState::NotRequired,
    }
}

/// The deterministic signals above the lint's floor, in the order they are
/// reported.
fn weaknesses(
    history: &EntryHistory,
    lexicon: &AboutLexicon,
    summary: &str,
) -> Vec<SummaryWeakness> {
    let text = &history.latest().text;
    let summary_terms = informative_tokens(summary).collect::<Vec<_>>();
    let text_terms = informative_tokens(text).count();
    let mut weaknesses = Vec::new();

    if summary_terms.len() <= SummaryWeakness::THIN_CEILING
        && text_terms
            >= summary_terms
                .len()
                .saturating_mul(SummaryWeakness::THIN_TEXT_RATIO)
    {
        weaknesses.push(SummaryWeakness::Thin {
            informative_terms: summary_terms.len(),
            text_terms,
        });
    }

    let repeated = lexicon.other_entries_repeating(summary);
    if repeated > 0 {
        weaknesses.push(SummaryWeakness::Repeated {
            other_entries: repeated,
        });
    }

    let others = lexicon.entries().saturating_sub(1);
    if lexicon.entries() >= SummaryWeakness::DISCRIMINATION_MINIMUM_ENTRIES
        && !summary_terms.is_empty()
        && summary_terms
            .iter()
            .all(|term| lexicon.other_entries_reached(term, true) * 2 > others)
    {
        weaknesses.push(SummaryWeakness::Undiscriminating {
            other_entries: others,
        });
    }

    if history.text_outlived_its_summary() {
        weaknesses.push(SummaryWeakness::Stale);
    }
    weaknesses
}

#[cfg(test)]
mod tests {
    use super::super::entry_revision::EntryRevision;
    use super::*;

    fn revision(text: &str, summary: Option<&str>) -> EntryRevision {
        EntryRevision {
            kind: "decision".to_string(),
            text: text.to_string(),
            summary: summary.map(str::to_string),
            summary_by: None,
        }
    }

    fn history(revisions: &[(&str, Option<&str>)]) -> EntryHistory {
        let mut revisions = revisions.iter();
        let (text, summary) = revisions.next().expect("a first write");
        let mut history = EntryHistory::new(
            "project:a".to_string(),
            "project:a:e1".to_string(),
            revision(text, *summary),
        );
        for (text, summary) in revisions {
            history.record(revision(text, *summary));
        }
        history
    }

    fn lexicon(entries: &[(&str, Option<&str>)]) -> AboutLexicon {
        let mut lexicon = AboutLexicon::default();
        for (text, summary) in entries {
            lexicon.admit(text, *summary);
        }
        lexicon
    }

    #[test]
    fn a_memory_an_english_question_cannot_reach_owes_a_rendering() {
        let text = "El despliegue se retrasó porque los auditores no firmaron.";

        let audited = judge(&history(&[(text, None)]), &lexicon(&[(text, None)]));

        assert_eq!(audited.state, SummaryState::Missing);
        assert_eq!(audited.text.as_deref(), Some(text));
    }

    #[test]
    fn an_english_memory_owes_nothing_and_carries_no_text_to_rewrite() {
        let text = "The rollout slipped because the auditors had not signed off.";

        let audited = judge(&history(&[(text, None)]), &lexicon(&[(text, None)]));

        assert_eq!(audited.state, SummaryState::NotRequired);
        assert_eq!(audited.text, None);
    }

    #[test]
    fn a_refused_summary_carries_the_lints_own_faults_and_no_weakness() {
        let text = "The v0.7.0 rollout slipped because the auditors had not signed off.";
        let summary = "The launch was postponed because the audit sign-off was missing.";

        let audited = judge(
            &history(&[(text, Some(summary))]),
            &lexicon(&[(text, Some(summary))]),
        );

        assert_eq!(audited.state, SummaryState::Refused);
        assert_eq!(
            audited.fault_sentences(),
            ["drops identifiers the text carries: v0.7.0"]
        );
        assert!(audited.weaknesses.is_empty());
    }

    #[test]
    fn a_rendering_on_the_lints_floor_for_a_long_text_is_thin() {
        let text = "El comité revisó el calendario, los presupuestos, las dependencias \
                    externas, los riesgos conocidos y las fechas de entrega antes de aprobar \
                    la fase siguiente del programa de migración.";
        let summary = "migration programme";

        let audited = judge(
            &history(&[(text, Some(summary))]),
            &lexicon(&[(text, Some(summary))]),
        );

        assert_eq!(audited.state, SummaryState::Stands);
        assert!(
            audited
                .weaknesses
                .iter()
                .any(|weakness| weakness.name() == "thin"),
            "{:?}",
            audited.weaknesses
        );
    }

    #[test]
    fn one_sentence_attached_to_a_batch_is_repeated() {
        let summary = "The rollout of v0.7.0 was reviewed by the auditors.";
        let text = "El punto uno del despliegue v0.7.0 quedó anotado.";
        let corpus = lexicon(&[
            (text, Some(summary)),
            (
                "El punto dos del despliegue v0.7.0 quedó anotado.",
                Some(summary),
            ),
        ]);

        let audited = judge(&history(&[(text, Some(summary))]), &corpus);

        assert!(
            audited
                .weaknesses
                .contains(&SummaryWeakness::Repeated { other_entries: 1 })
        );
    }

    /// Below the minimum, one neighbour is already "most of them", so the
    /// signal says nothing and is not reported.
    #[test]
    fn discrimination_is_not_judged_on_an_about_too_small_to_judge_it() {
        let text = "El despliegue del almacén compartido avanzó.";
        let summary = "The shared store rollout advanced.";
        let small = lexicon(&[(text, Some(summary)), (text, Some("Another rendering."))]);

        let audited = judge(&history(&[(text, Some(summary))]), &small);

        assert!(
            !audited
                .weaknesses
                .iter()
                .any(|weakness| weakness.name() == "undiscriminating"),
            "{:?}",
            audited.weaknesses
        );
    }

    #[test]
    fn a_summary_left_behind_by_a_later_rewrite_is_stale() {
        let summary = "The reserve valve froze during the night shift.";
        let now = "La bomba principal se detuvo durante el turno de noche.";

        let audited = judge(
            &history(&[
                ("La válvula de reserva se congeló de noche.", Some(summary)),
                (now, Some(summary)),
            ]),
            &lexicon(&[(now, Some(summary))]),
        );

        assert_eq!(audited.state, SummaryState::Stands);
        assert!(audited.weaknesses.contains(&SummaryWeakness::Stale));
        assert_eq!(audited.text.as_deref(), Some(now));
    }
}
