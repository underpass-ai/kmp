use std::path::Path;
use std::sync::Arc;

use super::shared_outcomes::SharedOutcomes;
use super::sqlite_verdict_book::SqliteVerdictBook;
use super::verdict_book_config::{VERDICT_BOOK_FILE, VerdictBookConfig};
use crate::serving::judgement_origin::JudgementOrigin;
use crate::serving::judgement_response::JudgementResponse;
use crate::serving::judgement_site::JudgementSite;
use crate::serving::ports::verdict_book::VerdictBook;
use crate::serving::verdict::Verdict;
use crate::serving::verdict_key::VerdictKey;
use crate::serving::verdict_template::VerdictTemplate;

/// What one provider call for the questions the book lacked came to.
type Flight = (Result<JudgementResponse, String>, JudgementOrigin);

const TARGET: &str = "kmp_mcp::verdict_book";

/// One store's verdict book and the judgements in flight against it, shared
/// by every call site of the process: a request whose missing verdicts are
/// already being asked waits for that answer instead of asking again.
pub(super) struct VerdictLedger {
    book: Arc<dyn VerdictBook>,
    pub(super) in_flight: SharedOutcomes<Flight>,
    /// Behind a judgement cassette (evaluation), a request the book answers
    /// only in part is sent whole: a cassette is keyed on the whole body,
    /// and one recorded by a binary without the book must keep replaying.
    /// Without a cassette only the questions the book lacks are sent.
    pub(super) whole_requests: bool,
}

impl VerdictLedger {
    pub(super) fn new(book: Arc<dyn VerdictBook>) -> Self {
        Self {
            book,
            in_flight: SharedOutcomes::new(0),
            whole_requests: false,
        }
    }

    /// The same ledger sending partly answered requests whole.
    pub(super) fn sending_whole_requests(mut self, whole: bool) -> Self {
        self.whole_requests = whole;
        self
    }

    /// The store's book, when its judgement can run: never without
    /// `typesafe.json`, so a store that never opted into Jev never gets the
    /// file. `Ok(None)` also when the store turned the book off.
    pub(super) fn load<T>(
        data_dir: &Path,
        judgement: &Result<Option<T>, String>,
        behind_cassette: bool,
    ) -> Result<Option<Arc<Self>>, String> {
        if !matches!(judgement, Ok(Some(_))) {
            return Ok(None);
        }
        let Some(max_bytes) = VerdictBookConfig::load(data_dir)? else {
            return Ok(None);
        };
        let book = SqliteVerdictBook::open(&data_dir.join(VERDICT_BOOK_FILE), max_bytes)?;
        let templates = JudgementSite::ALL.map(JudgementSite::template);
        retire_superseded(&book, &templates)?;
        Ok(Some(Arc::new(
            Self::new(Arc::new(book)).sending_whole_requests(behind_cassette),
        )))
    }

    /// The verdict held for each key; the book failing reads as holding
    /// none, so judging goes on and the failure is logged.
    pub(super) async fn read(&self, keys: Vec<VerdictKey>) -> Vec<Option<Verdict>> {
        let count = keys.len();
        let book = Arc::clone(&self.book);
        match tokio::task::spawn_blocking(move || book.read(&keys)).await {
            Ok(Ok(found)) => found,
            Ok(Err(error)) => {
                warn(&error);
                vec![None; count]
            }
            Err(_) => {
                warn("verdict book task failed");
                vec![None; count]
            }
        }
    }

    /// What the book holds for each key after keeping `verdicts`: the first
    /// verdict recorded wins. A book that cannot write hands the fresh
    /// verdicts back unchanged, and the failure is logged.
    pub(super) async fn record(&self, verdicts: Vec<(VerdictKey, Verdict)>) -> Vec<Verdict> {
        let book = Arc::clone(&self.book);
        let fresh = verdicts
            .iter()
            .map(|(_, verdict)| verdict.clone())
            .collect();
        match tokio::task::spawn_blocking(move || book.record(&verdicts)).await {
            Ok(Ok(held)) => held,
            Ok(Err(error)) => {
                warn(&error);
                fresh
            }
            Err(_) => {
                warn("verdict book task failed");
                fresh
            }
        }
    }
}

/// Drops the verdicts of every version older than the current one of each
/// template: a bumped template never answers with its old meaning, and its
/// old verdicts do not wait for collection to leave. Returns how many went.
fn retire_superseded(
    book: &dyn VerdictBook,
    templates: &[VerdictTemplate],
) -> Result<usize, String> {
    let mut dropped = 0;
    for template in templates {
        for version in 1..template.version {
            dropped += book.invalidate(
                &VerdictTemplate {
                    version,
                    ..*template
                }
                .prefix(),
            )?;
        }
    }
    Ok(dropped)
}

fn warn(error: &str) {
    tracing::warn!(target: TARGET, error, "verdict book unavailable; judging without it");
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::serving::judgement_answer::JudgementAnswer;
    use crate::serving::judgement_question::JudgementQuestion;
    use crate::serving::verdict_state_digest::StateDigest;

    #[test]
    fn no_book_is_created_without_a_working_judgement_or_when_turned_off() {
        let dir = tempfile::tempdir().expect("dir");
        let book = dir.path().join(VERDICT_BOOK_FILE);
        assert!(
            VerdictLedger::load(dir.path(), &Ok::<Option<()>, String>(None), false)
                .expect("off")
                .is_none()
        );
        assert!(
            VerdictLedger::load(
                dir.path(),
                &Err::<Option<()>, String>("no key".into()),
                false
            )
            .expect("broken opt-in")
            .is_none()
        );
        assert!(!book.exists());
        std::fs::write(dir.path().join("judgement-book.json"), r#"{"mode":"off"}"#)
            .expect("config");
        assert!(
            VerdictLedger::load(dir.path(), &Ok(Some(())), false)
                .expect("off")
                .is_none()
        );
        assert!(!book.exists());
        std::fs::remove_file(dir.path().join("judgement-book.json")).expect("remove");
        assert!(
            VerdictLedger::load(dir.path(), &Ok(Some(())), false)
                .expect("on")
                .is_some()
        );
        assert!(book.exists());
    }

    #[test]
    fn a_bumped_template_retires_its_older_versions() {
        let dir = tempfile::tempdir().expect("dir");
        let book =
            SqliteVerdictBook::open(&dir.path().join(VERDICT_BOOK_FILE), 1 << 20).expect("book");
        let question = JudgementQuestion::Noul {
            instructions: json!("q"),
        };
        let state = StateDigest::of(&json!("s"));
        let verdict =
            Verdict::of(&question, &JudgementAnswer::Noul { yes: 0.5 }, 1, 1).expect("fits");
        let at = |version| VerdictTemplate {
            id: "paths",
            version,
        };
        book.record(&[
            (
                VerdictKey::of("m", at(1), &question, &state),
                verdict.clone(),
            ),
            (
                VerdictKey::of("m", at(2), &question, &state),
                verdict.clone(),
            ),
            (
                VerdictKey::of("m", at(3), &question, &state),
                verdict.clone(),
            ),
        ])
        .expect("record");
        assert_eq!(retire_superseded(&book, &[at(3)]).expect("retire"), 2);
        let current = VerdictKey::of("m", at(3), &question, &state);
        assert_eq!(book.read(&[current]).expect("read"), vec![Some(verdict)]);
        assert_eq!(
            retire_superseded(&book, &JudgementSite::ALL.map(JudgementSite::template))
                .expect("every site is at its first version"),
            0
        );
    }
}
