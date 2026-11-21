use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use kmp_domain::language::LanguageVocabulary;
use kmp_proto_mapping::v1beta1::{DoubtBand, DoubtEntry, DoubtVerdicts};
use serde_json::json;
use sha2::{Digest, Sha256};

use super::ask_cross_language_config::{ASK_CROSS_LANGUAGE_FILE, AskCrossLanguageConfig};
use super::doubt_band_judge::doubt_judgement;
use crate::serving::doubt_outcome::DoubtOutcome;
use crate::serving::environment::judgement_deadline;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::judgement_response::JudgementResponse;
use crate::serving::judgement_site::JudgementSite;
use crate::serving::noul_criteria::NoulCriteria;
use crate::serving::ports::judgement_model::JudgementModel;

const TARGET: &str = "kmp_mcp::cross_language";

/// The key of the companion question of a rescue.
const COMPANION: &str = "any";

// Every word the judge sends lives here (TypeSafe: keep the wording in one
// place). One judgement per question, true means yes, no "or"; the `false`
// criterion names the situation that confuses it; the languages, which code
// already read, are stated as facts in named fields rather than hedged in
// the question.

/// The state's field for the reader's question, and for its language.
const ASKED: &str = "user_question";
const ASKED_LANGUAGE: &str = "user_question_language";
/// One question per passage: does it state what was asked?
const STATES: &str = "Does `passage` state the answer to `user_question`?";
const STATES_TRUE: &str = "the passage states the fact `user_question` asks for";
const STATES_FALSE: &str =
    "the passage is about the same subject but does not state the asked fact";
/// The companion of a rescue: can the question be answered from these
/// passages at all? Its own Noul, never a `none` option (as of Jev 1.14 the
/// companion usually works better).
const ANSWERABLE: &str = "Can `user_question` be answered from `passages`?";
const ANSWERABLE_TRUE: &str = "one of the passages states the fact `user_question` asks for";
const ANSWERABLE_FALSE: &str =
    "the passages are about the same subject but none of them states the asked fact";

/// Jev as the cross-language judge of `kmp_ask`, behind its own opt-in,
/// `ask-cross-language.json`, on top of `typesafe.json`.
///
/// One batch per ask that enters one of its two entries, through the
/// store's verdict book under its own site (`cross_language`): a veto asks
/// about each translated citation, a rescue about up to eight memories the
/// bridge reached in another language plus the companion. The first page
/// waits at most the site's deadline; past it, or when Jev fails, today's
/// deterministic answer stands with a warning while the judgement finishes
/// into the book for the next ask. A continuation never asks.
pub(super) struct CrossLanguageJudge {
    model: Arc<dyn JudgementModel>,
    config: AskCrossLanguageConfig,
    deadline: Option<Duration>,
}

impl CrossLanguageJudge {
    /// Off without `ask-cross-language.json`. With it, the store's TypeSafe
    /// opt-in must work too; otherwise the error says why and Ask stays
    /// deterministic.
    pub(super) fn load(
        data_dir: &Path,
        judgement: &Result<Option<Arc<dyn JudgementModel>>, String>,
    ) -> Result<Option<Arc<Self>>, String> {
        let bytes = match std::fs::read(data_dir.join(ASK_CROSS_LANGUAGE_FILE)) {
            Ok(bytes) if bytes.len() <= 8192 => bytes,
            Ok(_) => return Err("ask cross-language configuration exceeds 8192 bytes".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("cannot read ask cross-language configuration".into()),
        };
        let config: AskCrossLanguageConfig = serde_json::from_slice(&bytes)
            .map_err(|error| format!("invalid ask cross-language configuration: {error}"))?;
        config.validate()?;
        match judgement {
            Ok(Some(model)) => Ok(Some(Arc::new(Self::new(Arc::clone(model), config)))),
            Ok(None) => Err("ask-cross-language.json needs typesafe.json beside the store".into()),
            Err(error) => Err(error.clone()),
        }
    }

    pub(super) fn new(model: Arc<dyn JudgementModel>, config: AskCrossLanguageConfig) -> Self {
        Self {
            model,
            config,
            deadline: judgement_deadline(JudgementSite::CrossLanguage),
        }
    }

    /// The same judge with another first-page deadline (tests).
    #[cfg(test)]
    pub(super) fn with_deadline(mut self, deadline: Option<Duration>) -> Self {
        self.deadline = deadline;
        self
    }

    pub(super) fn veto(&self) -> bool {
        self.config.veto()
    }

    pub(super) fn rescue(&self) -> bool {
        self.config.rescue()
    }

    pub(super) fn keep_permille(&self) -> u16 {
        self.config.keep_permille()
    }

    pub(super) fn rescue_permille(&self) -> u16 {
        self.config.rescue_permille()
    }

    /// Asks Jev about a cross-language band's passages, within the deadline.
    pub(super) async fn judge(&self, question: &str, band: &DoubtBand) -> DoubtOutcome {
        let started = Instant::now();
        let rescue = band.entry == DoubtEntry::CrossLanguageUnknown;
        let request = self.request(question, band, rescue);
        let model = Arc::clone(&self.model);
        let asked = request.clone();
        // Detached, so a judgement past the deadline still lands in the
        // book for the next ask.
        let judged = tokio::spawn(async move { model.evaluate(&asked).await });
        let joined = match self.deadline {
            Some(deadline) => match tokio::time::timeout(deadline, judged).await {
                Ok(joined) => joined,
                Err(_) => {
                    report(band, None, None, "deadline", started);
                    return unavailable(format!(
                        "Jev did not answer within {} ms",
                        deadline.as_millis()
                    ));
                }
            },
            None => judged.await,
        };
        let response = match joined {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => {
                report(band, None, None, "error", started);
                return unavailable(error);
            }
            Err(_) => {
                report(band, None, None, "error", started);
                return unavailable("the judgement task failed".into());
            }
        };
        match self.verdicts(band, &response, rescue) {
            Ok((verdicts, judged, companion)) => {
                report(band, Some(&judged), companion, "judged", started);
                DoubtOutcome {
                    verdicts: (!verdicts.is_empty()).then_some(verdicts),
                    warning: None,
                }
            }
            Err(error) => {
                report(band, None, None, "error", started);
                unavailable(error)
            }
        }
    }

    /// The verdicts the answer applies, what each passage was judged, and
    /// the companion's judgement of a rescue.
    #[allow(clippy::type_complexity)]
    fn verdicts(
        &self,
        band: &DoubtBand,
        response: &JudgementResponse,
        rescue: bool,
    ) -> Result<(DoubtVerdicts, Vec<Option<u16>>, Option<u16>), String> {
        let template = JudgementSite::CrossLanguage.template();
        // The veto reads its own line (`keep_at`, [`DoubtVerdicts::with_keep_line`]);
        // the band's B1 line is never reached here.
        let mut verdicts = DoubtVerdicts::new(
            band.entry,
            self.model.model().to_string(),
            format!("{}.v{}", template.id, template.version),
            1_000,
            None,
        )
        .map_err(|_| "invalid cross-language thresholds".to_string())?;
        if band.entry == DoubtEntry::TranslatedAnswer {
            verdicts = verdicts
                .with_keep_line(self.config.keep_permille())
                .map_err(|_| "invalid cross-language veto".to_string())?;
        }
        let companion = if rescue {
            let companion = response
                .answers
                .get(COMPANION)
                .and_then(doubt_judgement)
                .ok_or("Jev gave no answer to the companion question")?
                .answers;
            verdicts = verdicts
                .with_rescue(self.config.rescue_permille(), companion)
                .map_err(|_| "invalid cross-language rescue".to_string())?;
            Some(companion)
        } else {
            None
        };
        let mut judged = Vec::with_capacity(band.passages.len());
        for (n, passage) in band.passages.iter().enumerate() {
            let judgement = response
                .answers
                .get(&passage_key(n))
                .filter(|answer| matches!(answer, JudgementAnswer::Noul { .. }))
                .and_then(doubt_judgement);
            match judgement {
                Some(judgement)
                    if verdicts
                        .judge(&passage.id, &passage.text_sha256, judgement)
                        .is_ok() =>
                {
                    judged.push(Some(judgement.answers));
                }
                _ => judged.push(None),
            }
        }
        Ok((verdicts, judged, companion))
    }

    fn request(&self, question: &str, band: &DoubtBand, rescue: bool) -> JudgementRequest {
        let excerpt = self.config.excerpt_chars();
        let excerpts = band
            .passages
            .iter()
            .map(|passage| passage.text.chars().take(excerpt).collect::<String>())
            .collect::<Vec<_>>();
        let languages = LanguageVocabulary::shipped();
        let written_in = band
            .passages
            .iter()
            .map(|passage| languages.leans_in(&passage.text))
            .collect::<Vec<_>>();
        let mut questions = excerpts
            .iter()
            .zip(&written_in)
            .enumerate()
            .map(|(n, (passage, language))| {
                let mut instructions = json!({"passage": passage, "question": STATES});
                if let Some(language) = language {
                    instructions["passage_language"] = json!(language);
                }
                (
                    passage_key(n),
                    JudgementQuestion::CriteriaNoul {
                        instructions,
                        criteria: NoulCriteria::new(STATES_TRUE, STATES_FALSE),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        if rescue {
            let mut instructions = json!({"passages": excerpts, "question": ANSWERABLE});
            if let Some(Some(language)) = written_in.first()
                && written_in.iter().all(|other| other == &Some(*language))
            {
                instructions["passages_language"] = json!(language);
            }
            questions.insert(
                COMPANION.to_string(),
                JudgementQuestion::CriteriaNoul {
                    instructions,
                    criteria: NoulCriteria::new(ANSWERABLE_TRUE, ANSWERABLE_FALSE),
                },
            );
        }
        let mut state = serde_json::Map::new();
        state.insert(ASKED.to_string(), json!(question));
        if let Some(language) = languages.leans_in(question) {
            state.insert(ASKED_LANGUAGE.to_string(), json!(language));
        }
        JudgementRequest {
            state: serde_json::Value::Object(state),
            questions,
        }
    }
}

fn passage_key(n: usize) -> String {
    format!("p{n}")
}

fn unavailable(error: String) -> DoubtOutcome {
    DoubtOutcome {
        verdicts: None,
        warning: Some(format!(
            "cross-language judge unavailable; the deterministic answer stands: {error}"
        )),
    }
}

/// One debug line per band on `kmp_mcp::cross_language`
/// (`RUST_LOG=kmp_mcp::cross_language=debug`): the entry, how it ended, the
/// companion's thousandths for a rescue, and per passage a digest of its
/// entry ref (never the ref or the text), whether it is cited and the
/// thousandths of stating the answer, so the bench can score the judge
/// against gold refs without the log carrying stored words.
fn report(
    band: &DoubtBand,
    judged: Option<&[Option<u16>]>,
    companion: Option<u16>,
    status: &str,
    started: Instant,
) {
    if !tracing::enabled!(target: TARGET, tracing::Level::DEBUG) {
        return;
    }
    let passages = band
        .passages
        .iter()
        .enumerate()
        .map(|(n, passage)| {
            let digest = format!("{:x}", Sha256::digest(passage.entry_ref.as_bytes()));
            let states = judged
                .and_then(|judged| judged.get(n).copied().flatten())
                .map_or_else(|| "-".to_string(), |states| states.to_string());
            format!(
                "{}:{}:{states}",
                &digest[..16],
                if passage.in_core { "c" } else { "p" }
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    tracing::debug!(
        target: TARGET,
        event = "kmp_cross_language",
        entry = band.entry.as_str(),
        status,
        passages = band.passages.len(),
        judged = passages.as_str(),
        companion = companion.map_or(-1, i64::from),
        elapsed_us = started.elapsed().as_micros() as u64,
        "cross-language judge"
    );
}
