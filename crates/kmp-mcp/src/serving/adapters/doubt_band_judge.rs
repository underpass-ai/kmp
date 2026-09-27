use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use kmp_proto_mapping::v1beta1::{DoubtBand, DoubtJudgement, DoubtVerdicts};
use serde_json::json;
use sha2::{Digest, Sha256};

use super::ask_judge_config::{ASK_JUDGE_FILE, AskJudgeConfig};
use super::ask_judge_question::AskJudgeQuestion;
use crate::serving::doubt_outcome::DoubtOutcome;
use crate::serving::environment::judgement_deadline;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::judgement_site::JudgementSite;
use crate::serving::ports::judgement_model::JudgementModel;

const TARGET: &str = "kmp_mcp::doubt_band";

/// A passage's verdict, if judged, with a graded question's expected grade.
type JudgedPassage = Option<(DoubtJudgement, Option<u16>)>;

/// The ask doubt band's judge (DESIGN L4 4f, option B), behind its own
/// opt-in, `ask-judge.json`, on top of `typesafe.json`.
///
/// One batch per ask in the band: one question per passage — does it answer
/// the question? — about at most eight admitted passages, through the
/// store's verdict book. The first page waits at most the site's deadline;
/// past it the deterministic answer stands, with a warning, while the
/// judgement finishes into the book for the next ask. A continuation never
/// asks: it reads the first page's frozen answer.
pub(super) struct DoubtBandJudge {
    model: Arc<dyn JudgementModel>,
    config: AskJudgeConfig,
    deadline: Option<Duration>,
}

impl DoubtBandJudge {
    /// Off without `ask-judge.json`. With it, the store's TypeSafe opt-in
    /// must work too; otherwise the error says why and Ask stays
    /// deterministic.
    pub(super) fn load(
        data_dir: &Path,
        judgement: &Result<Option<Arc<dyn JudgementModel>>, String>,
    ) -> Result<Option<Arc<Self>>, String> {
        let bytes = match std::fs::read(data_dir.join(ASK_JUDGE_FILE)) {
            Ok(bytes) if bytes.len() <= 8192 => bytes,
            Ok(_) => return Err("ask judge configuration exceeds 8192 bytes".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("cannot read ask judge configuration".into()),
        };
        let config: AskJudgeConfig =
            serde_json::from_slice(&bytes).map_err(|_| "invalid ask judge configuration")?;
        config.validate()?;
        match judgement {
            Ok(Some(model)) => Ok(Some(Arc::new(Self::new(Arc::clone(model), config)))),
            Ok(None) => Err("ask-judge.json needs typesafe.json beside the store".into()),
            Err(error) => Err(error.clone()),
        }
    }

    pub(super) fn new(model: Arc<dyn JudgementModel>, config: AskJudgeConfig) -> Self {
        Self {
            model,
            config,
            deadline: judgement_deadline(JudgementSite::DoubtBand),
        }
    }

    /// The same judge with another first-page deadline (tests).
    #[cfg(test)]
    pub(super) fn with_deadline(mut self, deadline: Option<Duration>) -> Self {
        self.deadline = deadline;
        self
    }

    /// The margin, in tenths, under which an answer is in doubt.
    pub(super) fn margin_tenths(&self) -> i64 {
        self.config.margin_tenths()
    }

    /// Asks the judge about the band's passages, within the deadline.
    pub(super) async fn judge(&self, question: &str, band: &DoubtBand) -> DoubtOutcome {
        let started = Instant::now();
        let request = self.request(question, band);
        let model = Arc::clone(&self.model);
        let asked = request.clone();
        // Detached, so a judgement past the deadline still lands in the
        // book for the next ask.
        let judged = tokio::spawn(async move { model.evaluate(&asked).await });
        let joined = match self.deadline {
            Some(deadline) => match tokio::time::timeout(deadline, judged).await {
                Ok(joined) => joined,
                Err(_) => {
                    report(band, None, "deadline", started);
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
                report(band, None, "error", started);
                return unavailable(error);
            }
            Err(_) => {
                report(band, None, "error", started);
                return unavailable("the judgement task failed".into());
            }
        };
        let mut verdicts = match DoubtVerdicts::new(
            band.entry,
            self.model.model().to_string(),
            format!(
                "{}.v{}",
                JudgementSite::DoubtBand.template().id,
                JudgementSite::DoubtBand.template().version
            ),
            self.config.veto_permille(),
            self.config.promote_permille(),
        ) {
            Ok(verdicts) => verdicts,
            Err(_) => return unavailable("invalid doubt band thresholds".into()),
        };
        let mut judged = Vec::new();
        for (n, passage) in band.passages.iter().enumerate() {
            let answer = response.answers.get(&format!("d{n}"));
            let judgement = answer.and_then(doubt_judgement);
            if let Some(judgement) = judgement
                && verdicts
                    .judge(&passage.id, &passage.text_sha256, judgement)
                    .is_ok()
            {
                judged.push(Some((judgement, answer.and_then(graded))));
            } else {
                judged.push(None);
            }
        }
        report(band, Some(&judged), "judged", started);
        DoubtOutcome {
            verdicts: (!verdicts.is_empty()).then_some(verdicts),
            warning: None,
        }
    }

    fn request(&self, question: &str, band: &DoubtBand) -> JudgementRequest {
        let excerpt = self.config.excerpt_chars();
        let questions = band
            .passages
            .iter()
            .enumerate()
            .map(|(n, passage)| {
                let passage = passage.text.chars().take(excerpt).collect::<String>();
                let asked = match self.config.question() {
                    AskJudgeQuestion::Noul => JudgementQuestion::Noul {
                        instructions: json!({
                            "passage": passage,
                            "question": "Does `passage` answer the question in the state?",
                        }),
                    },
                    AskJudgeQuestion::Score => JudgementQuestion::Score {
                        instructions: json!({
                            "passage": passage,
                            "question": "How well does `passage` answer the question in the state?",
                        }),
                        levels: AskJudgeQuestion::LEVELS.map(String::from).to_vec(),
                    },
                };
                (format!("d{n}"), asked)
            })
            .collect::<BTreeMap<_, _>>();
        JudgementRequest {
            state: json!(question),
            questions,
        }
    }
}

/// A judge's answer as thousandths of answering and of not answering. A
/// graded answer counts only its first grade as answering and its last two
/// as not; "in part" is neither.
pub(super) fn doubt_judgement(answer: &JudgementAnswer) -> Option<DoubtJudgement> {
    let permille = |p: f64| (p.clamp(0.0, 1.0) * 1_000.0).round() as u16;
    match answer {
        JudgementAnswer::Noul { yes } => Some(DoubtJudgement {
            answers: permille(*yes),
            not_answers: 1_000 - permille(*yes),
        }),
        JudgementAnswer::Score { probabilities, .. }
            if probabilities.len() == AskJudgeQuestion::LEVELS.len() =>
        {
            Some(DoubtJudgement {
                answers: permille(probabilities[0]),
                not_answers: permille(probabilities[2] + probabilities[3]),
            })
        }
        _ => None,
    }
}

/// A graded answer's expected grade as thousandths of the way from the
/// worst level to the best, for telemetry: 1000 answers, 0 unrelated.
pub(super) fn graded(answer: &JudgementAnswer) -> Option<u16> {
    match answer {
        JudgementAnswer::Score {
            score,
            probabilities,
            ..
        } if probabilities.len() > 1 => {
            let top = (probabilities.len() - 1) as f64;
            Some(((1.0 - score / top).clamp(0.0, 1.0) * 1_000.0).round() as u16)
        }
        _ => None,
    }
}

fn unavailable(error: String) -> DoubtOutcome {
    DoubtOutcome {
        verdicts: None,
        warning: Some(format!(
            "doubt band unavailable; the deterministic answer stands: {error}"
        )),
    }
}

/// One debug line per band on `kmp_mcp::doubt_band`
/// (`RUST_LOG=kmp_mcp::doubt_band=debug`): the entry, how it ended, and per
/// passage a digest of its entry ref (never the ref or the text), whether it
/// is cited and the judged thousandths (answering/not, and for a graded
/// question the expected grade), so the bench can score the judge against
/// gold refs without the log carrying stored words.
fn report(band: &DoubtBand, judged: Option<&[JudgedPassage]>, status: &str, started: Instant) {
    if !tracing::enabled!(target: TARGET, tracing::Level::DEBUG) {
        return;
    }
    let passages = band
        .passages
        .iter()
        .enumerate()
        .map(|(n, passage)| {
            let digest = format!("{:x}", Sha256::digest(passage.entry_ref.as_bytes()));
            let judged = judged
                .and_then(|judged| judged.get(n).copied().flatten())
                .map(|(j, grade)| match grade {
                    Some(grade) => format!("{}/{}/{grade}", j.answers, j.not_answers),
                    None => format!("{}/{}", j.answers, j.not_answers),
                })
                .unwrap_or_else(|| "-".into());
            format!(
                "{}:{}:{}",
                &digest[..16],
                if passage.in_core { "c" } else { "p" },
                judged
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    tracing::debug!(
        target: TARGET,
        event = "kmp_doubt_band",
        entry = band.entry.as_str(),
        margin = band.margin.unwrap_or(i64::MIN),
        status,
        passages = band.passages.len(),
        judged = passages.as_str(),
        elapsed_us = started.elapsed().as_micros() as u64,
        "doubt band"
    );
}
