use kmp_domain::language::{KERNEL_LANGUAGE, identifiers, informative_tokens};

use super::morphology::Morphology;
use super::search_probe_terms::SearchProbeTerms;
use super::search_terms::{concept_key, informative_terms};

/// A read-only window onto the words the ranker compares.
///
/// Benchmarks and diagnostics need to know what the kernel's tokenizer made
/// of a question or a memory without reimplementing it, because a copy drifts
/// the day the kernel changes. This facade exposes the same functions the
/// ranker calls, with the morphology chosen the way the ranker chooses it:
/// read once from the about's own texts, falling back to the kernel's search
/// language only when the about carries an English search summary.
///
/// It reads nothing from a store and changes nothing; it is pure over its
/// inputs.
pub struct SearchProbe {
    language: Option<String>,
    morphology: Morphology,
}

impl SearchProbe {
    /// The probe for an about whose memory reads as `about_texts`.
    ///
    /// `carries_search_summary` mirrors the ranker's fallback: a store whose
    /// language cannot be read is stemmed in the kernel's search language
    /// only if it carries a linted English summary to land on.
    pub fn from_about_texts<'a>(
        about_texts: impl IntoIterator<Item = &'a str>,
        carries_search_summary: bool,
    ) -> Self {
        let store_language = Morphology::read_language(about_texts);
        let language =
            store_language.or_else(|| carries_search_summary.then(|| KERNEL_LANGUAGE.to_string()));
        let morphology = Morphology::for_language(language.as_deref());
        Self {
            language,
            morphology,
        }
    }

    /// The language the probe stems in, or `None` when it keeps exact
    /// matching. A language named here but without a Snowball stemmer still
    /// stems nothing.
    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    /// What the kernel's tokenizer makes of `text` under this probe.
    pub fn probe(&self, text: &str) -> SearchProbeTerms {
        let informative = informative_tokens(text).collect::<std::collections::BTreeSet<_>>();
        let concept_keys = informative
            .iter()
            .map(|term| concept_key(term).to_string())
            .collect();
        let search_keys = informative_terms(text, &self.morphology);
        SearchProbeTerms {
            informative_terms: informative,
            concept_keys,
            search_keys,
            identifiers: identifiers(text),
            compound_identifiers: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use kmp_proto::v1beta1::MemoryEvidence;

    use super::super::answer_candidate_terms::AnswerCandidateTerms;
    use super::super::answer_recall_context::AnswerRecallContext;
    use super::*;

    /// (about texts, whether the about carries a search summary, text,
    /// expected language). Spanish, English, mixed and unreadable abouts, so
    /// every branch of the morphology choice is compared against the ranker.
    const FIXTURES: &[(&[&str], bool, &str, Option<&str>)] = &[
        (
            &[
                "El despliegue de la pasarela se congelo durante la auditoria.",
                "La reunion semanal se movio a las diez de la manana.",
            ],
            false,
            "Desplegamos las válvulas nuevas de KMP-469 antes de la auditoría.",
            Some("spanish"),
        ),
        (
            &[
                "The deployment of the gateway was frozen during the audit.",
                "The weekly meeting moved to ten in the morning.",
            ],
            false,
            "We deployed the relocated valves for PR #83 and fixed the ranking.",
            Some("english"),
        ),
        (
            &[
                "The deployment of the gateway was frozen and the audit was in the way.",
                "El despliegue de la pasarela se congelo por la auditoria de la semana.",
            ],
            false,
            "Deployments of the gateways stayed frozen.",
            None,
        ),
        (
            &[
                "The deployment of the gateway was frozen and the audit was in the way.",
                "El despliegue de la pasarela se congelo por la auditoria de la semana.",
            ],
            true,
            "Deployments of the gateways stayed frozen.",
            Some("english"),
        ),
        (
            &["Valkey."],
            false,
            "Valkey stores the sqlite backend data.",
            None,
        ),
    ];

    fn candidate_terms(probe: &SearchProbe, text: &str) -> AnswerCandidateTerms {
        let context = AnswerRecallContext {
            morphology: Morphology::for_language(probe.language()),
            ..AnswerRecallContext::default()
        };
        let item = MemoryEvidence {
            text: text.to_string(),
            ..MemoryEvidence::default()
        };
        AnswerCandidateTerms::from_evidence(&item, &context)
    }

    #[test]
    fn search_keys_are_the_text_terms_the_ranker_builds() {
        for (about, summary, text, language) in FIXTURES {
            let probe = SearchProbe::from_about_texts(about.iter().copied(), *summary);
            assert_eq!(probe.language(), *language, "language for {text:?}");

            let probed = probe.probe(text);
            let ranked = candidate_terms(&probe, text);

            assert_eq!(probed.search_keys, ranked.text, "text terms for {text:?}");
            assert_eq!(probed.search_keys, ranked.content, "content for {text:?}");
            assert!(probed.compound_identifiers.is_empty());
        }
    }

    #[test]
    fn layers_show_which_step_changed_a_word() {
        let probe = SearchProbe::from_about_texts(
            ["The deployment of the gateway was frozen during the audit."],
            false,
        );
        let probed = probe.probe("The old sqlite store was relocated and deployed.");

        assert!(probed.informative_terms.contains("relocated"));
        assert!(!probed.informative_terms.contains("the"));
        assert!(probed.concept_keys.contains("concept:movement"));
        assert!(probed.concept_keys.contains("concept:storage-engine"));
        assert!(probed.concept_keys.contains("deployed"));
        assert!(probed.search_keys.contains("concept:historical"));
        assert!(probed.search_keys.contains("deploy"));
        assert!(!probed.search_keys.contains("deployed"));
    }

    #[test]
    fn identifiers_come_from_the_domain_reader() {
        let probe = SearchProbe::from_about_texts(["Valkey."], false);
        let probed = probe.probe("Ticket KMP-469 moved to PR #83.");

        assert_eq!(
            probed.identifiers,
            identifiers("Ticket KMP-469 moved to PR #83.")
        );
        assert_eq!(probe.language(), None);
        assert_eq!(
            probed.search_keys,
            probed
                .informative_terms
                .iter()
                .map(|term| concept_key(term).to_string())
                .collect::<BTreeSet<_>>()
        );
    }

    #[test]
    fn an_empty_text_yields_empty_layers() {
        let probed = SearchProbe::from_about_texts([], false).probe("");
        assert_eq!(probed, SearchProbeTerms::default());
    }
}
