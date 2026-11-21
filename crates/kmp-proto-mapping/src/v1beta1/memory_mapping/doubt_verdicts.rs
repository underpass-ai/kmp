use std::collections::BTreeMap;

use kmp_proto::v1beta1::MemoryEvidence;
use sha2::{Digest, Sha256};

use super::doubt_entry::DoubtEntry;
use super::doubt_judgement::DoubtJudgement;
use super::scalars::{ProtoMappingResult, invalid_argument};
use super::stated_reading::StatedReading;

/// A memory the deterministic reading cited and a judge vetoed: it stays in
/// `proof.evidence`, out of the core, marked with the model that said so.
pub(super) const JUDGED_OUT_KEY: &str = "judged_out";
/// A memory the judge promoted into the core: the model that did.
pub(super) const JUDGED_BY_KEY: &str = "judged_by";
/// The judge's probability behind either mark, in thousandths: of not
/// answering for a veto, of answering for a promotion.
pub(super) const JUDGED_PERMILLE_KEY: &str = "judged_permille";
/// The prompt template, with its version, the judgement answered.
pub(super) const JUDGED_TEMPLATE_KEY: &str = "judged_template";

/// A judge's verdicts on the passages of one doubt band, and what the store
/// lets them do (DESIGN L4 4f, option B).
///
/// - **B1, veto.** A cited memory the judge finds at least `veto_at`
///   likely not to answer leaves the core. It can only take answers away.
/// - **B2, promotion**, only when `promote_at` is set: an admitted memory
///   that passed the anchored gate, judged at least `promote_at` likely to
///   answer, joins the core, or turns an UNKNOWN whose anchor was found into
///   an answer. It never admits an absent anchor or anything the selection
///   did not admit, and it never writes text: the citation is the stored
///   memory, marked with the model, the probability and the template.
///
/// - **Cross-language rescue**, only on a [`DoubtEntry::CrossLanguageUnknown`]
///   band and only with [`Self::with_rescue`]: the one passage the judge
///   finds likeliest to state the answer, when it and the companion
///   judgement ("can the question be answered from these passages?") are
///   both at least `rescue_at`, joins the core of an UNKNOWN. It is the
///   only promotion that may cite a memory the question reached through the
///   lexical bridge rather than in its own words.
///
/// Verdicts are keyed by evidence id and the SHA-256 of the exact text the
/// judge read, so a memory whose text changed since is not judged.
#[derive(Debug, Clone)]
pub struct DoubtVerdicts {
    entry: DoubtEntry,
    model: String,
    template: String,
    veto_at: u16,
    promote_at: Option<u16>,
    /// The rescue line and the companion's judgement, in thousandths.
    rescue: Option<(u16, u16)>,
    /// The cross-language veto's line: a citation is kept only when judged
    /// at least this likely to state the answer (`StatedReading`).
    keep_at: Option<u16>,
    judged: BTreeMap<(String, String), DoubtJudgement>,
}

impl DoubtVerdicts {
    /// Thresholds in thousandths. A promotion bar and a veto bar that a
    /// single passage could clear together are refused.
    pub fn new(
        entry: DoubtEntry,
        model: String,
        template: String,
        veto_at: u16,
        promote_at: Option<u16>,
    ) -> ProtoMappingResult<Self> {
        let valid = !model.trim().is_empty()
            && model.len() <= 256
            && !template.trim().is_empty()
            && template.len() <= 64
            && (1..=1000).contains(&veto_at)
            && promote_at.is_none_or(|promote| {
                (1..=1000).contains(&promote) && u32::from(promote) + u32::from(veto_at) > 1000
            });
        if !valid {
            return Err(invalid_argument("invalid doubt band verdicts"));
        }
        Ok(Self {
            entry,
            model,
            template,
            veto_at,
            promote_at,
            rescue: None,
            keep_at: None,
            judged: BTreeMap::new(),
        })
    }

    /// The cross-language veto: a translated citation stays only when it
    /// reads yes (`StatedReading`) at `keep_at` thousandths; one that is
    /// unsettled or absent leaves. Only on a [`DoubtEntry::TranslatedAnswer`]
    /// band.
    pub fn with_keep_line(mut self, keep_at: u16) -> ProtoMappingResult<Self> {
        if self.entry != DoubtEntry::TranslatedAnswer
            || !(StatedReading::NO_AT + 1..=1000).contains(&keep_at)
        {
            return Err(invalid_argument("invalid cross-language veto"));
        }
        self.keep_at = Some(keep_at);
        Ok(self)
    }

    /// The cross-language rescue: `rescue_at` is the line both the best
    /// passage and `companion` (how likely the question can be answered
    /// from the passages at all) must reach, in thousandths. Only on a
    /// [`DoubtEntry::CrossLanguageUnknown`] band without B2 promotion.
    pub fn with_rescue(mut self, rescue_at: u16, companion: u16) -> ProtoMappingResult<Self> {
        if self.entry != DoubtEntry::CrossLanguageUnknown
            || self.promote_at.is_some()
            || !(StatedReading::NO_AT + 1..=1000).contains(&rescue_at)
            || companion > 1000
        {
            return Err(invalid_argument("invalid cross-language rescue"));
        }
        self.rescue = Some((rescue_at, companion));
        Ok(self)
    }

    /// Records the judgement of the passage `id` whose text hashes to
    /// `text_sha256`. Probabilities above 1000 are refused.
    pub fn judge(
        &mut self,
        id: &str,
        text_sha256: &str,
        judgement: DoubtJudgement,
    ) -> ProtoMappingResult<()> {
        if judgement.answers > 1000
            || judgement.not_answers > 1000
            || text_sha256.len() != 64
            || !text_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(invalid_argument("invalid doubt band judgement"));
        }
        self.judged.insert(
            (id.to_string(), text_sha256.to_ascii_lowercase()),
            judgement,
        );
        Ok(())
    }

    pub fn entry(&self) -> DoubtEntry {
        self.entry
    }

    pub fn len(&self) -> usize {
        self.judged.len()
    }

    pub fn is_empty(&self) -> bool {
        self.judged.is_empty()
    }

    pub(super) fn model(&self) -> &str {
        &self.model
    }

    fn judgement(&self, item: &MemoryEvidence) -> Option<DoubtJudgement> {
        let digest = format!("{:x}", Sha256::digest(item.text.as_bytes()));
        self.judged.get(&(item.id.clone(), digest)).copied()
    }

    /// The veto on a cited memory, as the probability it does not answer.
    pub(super) fn vetoes(&self, item: &MemoryEvidence) -> Option<u16> {
        if let Some(keep_at) = self.keep_at {
            return self
                .judgement(item)
                .filter(|judged| {
                    StatedReading::of(judged.answers, keep_at) != StatedReading::States
                })
                .map(|judged| judged.not_answers);
        }
        self.judgement(item)
            .map(|judged| judged.not_answers)
            .filter(|not_answers| *not_answers >= self.veto_at)
    }

    /// The promotion of an admitted memory, as the probability it answers.
    pub(super) fn promotes(&self, item: &MemoryEvidence) -> Option<u16> {
        if let Some((rescue_at, companion)) = self.rescue {
            return self.rescues(item, rescue_at, companion);
        }
        let promote_at = self.promote_at?;
        self.judgement(item)
            .map(|judged| judged.answers)
            .filter(|answers| *answers >= promote_at)
    }

    /// Whether the judge vetoes `item`, were the answer to cite it.
    pub fn is_vetoed(&self, item: &MemoryEvidence) -> bool {
        self.vetoes(item).is_some()
    }

    /// Whether a cross-language rescue cites `item`.
    pub fn is_rescued(&self, item: &MemoryEvidence) -> bool {
        self.is_rescue() && self.promotes(item).is_some()
    }

    /// Whether these verdicts are a cross-language rescue, which may cite a
    /// memory the bridge reached.
    pub(super) fn is_rescue(&self) -> bool {
        self.rescue.is_some()
    }

    /// The rescue of `item`: it is the passage judged likeliest to state the
    /// answer (ties to the lowest id and digest), and it and the companion
    /// both reach the line.
    fn rescues(&self, item: &MemoryEvidence, rescue_at: u16, companion: u16) -> Option<u16> {
        if StatedReading::of(companion, rescue_at) != StatedReading::States {
            return None;
        }
        let best =
            self.judged
                .iter()
                .fold(
                    None::<(&(String, String), u16)>,
                    |best, (key, judged)| match best {
                        Some((_, answers)) if answers >= judged.answers => best,
                        _ => Some((key, judged.answers)),
                    },
                )?;
        let digest = format!("{:x}", Sha256::digest(item.text.as_bytes()));
        (best.0.0 == item.id
            && best.0.1 == digest
            && StatedReading::of(best.1, rescue_at) == StatedReading::States)
            .then_some(best.1)
    }

    /// Marks a vetoed citation, which stays in the proof.
    pub(super) fn mark_out(&self, item: MemoryEvidence, permille: u16) -> MemoryEvidence {
        self.mark(item, JUDGED_OUT_KEY, permille)
    }

    /// Marks a promoted citation.
    pub(super) fn mark_in(&self, item: MemoryEvidence, permille: u16) -> MemoryEvidence {
        self.mark(item, JUDGED_BY_KEY, permille)
    }

    fn mark(&self, mut item: MemoryEvidence, key: &str, permille: u16) -> MemoryEvidence {
        item.metadata.insert(key.to_string(), self.model.clone());
        item.metadata
            .insert(JUDGED_PERMILLE_KEY.to_string(), permille.to_string());
        item.metadata
            .insert(JUDGED_TEMPLATE_KEY.to_string(), self.template.clone());
        item
    }

    /// Whether a memory carries a promotion mark.
    pub(super) fn was_promoted(item: &MemoryEvidence) -> bool {
        item.metadata.contains_key(JUDGED_BY_KEY)
    }

    /// Whether a memory carries a veto mark.
    pub(super) fn was_vetoed(item: &MemoryEvidence) -> bool {
        item.metadata.contains_key(JUDGED_OUT_KEY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, text: &str) -> MemoryEvidence {
        MemoryEvidence {
            id: id.into(),
            text: text.into(),
            ..MemoryEvidence::default()
        }
    }

    fn sha(text: &str) -> String {
        format!("{:x}", Sha256::digest(text.as_bytes()))
    }

    fn verdicts(promote_at: Option<u16>) -> DoubtVerdicts {
        let mut verdicts = DoubtVerdicts::new(
            DoubtEntry::NarrowMargin,
            "jev-1.14.0".into(),
            "doubt_band.v1".into(),
            800,
            promote_at,
        )
        .expect("valid");
        verdicts
            .judge(
                "entry:a",
                &sha("alpha"),
                DoubtJudgement {
                    answers: 100,
                    not_answers: 900,
                },
            )
            .expect("judged");
        verdicts
            .judge(
                "entry:b",
                &sha("beta"),
                DoubtJudgement {
                    answers: 950,
                    not_answers: 50,
                },
            )
            .expect("judged");
        verdicts
    }

    #[test]
    fn a_veto_and_a_promotion_read_the_judged_identity_only() {
        let verdicts = verdicts(Some(900));
        assert_eq!(verdicts.vetoes(&item("entry:a", "alpha")), Some(900));
        assert_eq!(verdicts.vetoes(&item("entry:b", "beta")), None);
        assert_eq!(
            verdicts.vetoes(&item("entry:a", "alpha changed")),
            None,
            "a text that changed since was not judged"
        );
        assert_eq!(verdicts.promotes(&item("entry:b", "beta")), Some(950));
        assert_eq!(verdicts.promotes(&item("entry:a", "alpha")), None);
        assert_eq!(verdicts.len(), 2);
        assert!(!verdicts.is_empty());
        assert_eq!(verdicts.entry(), DoubtEntry::NarrowMargin);
    }

    #[test]
    fn without_a_promotion_bar_nothing_is_promoted() {
        let verdicts = verdicts(None);
        assert_eq!(verdicts.promotes(&item("entry:b", "beta")), None);
    }

    #[test]
    fn marks_name_the_model_the_probability_and_the_template() {
        let verdicts = verdicts(Some(900));
        let out = verdicts.mark_out(item("entry:a", "alpha"), 900);
        assert_eq!(out.metadata[JUDGED_OUT_KEY], "jev-1.14.0");
        assert_eq!(out.metadata[JUDGED_PERMILLE_KEY], "900");
        assert_eq!(out.metadata[JUDGED_TEMPLATE_KEY], "doubt_band.v1");
        assert!(DoubtVerdicts::was_vetoed(&out));
        assert!(!DoubtVerdicts::was_promoted(&out));
        let promoted = verdicts.mark_in(item("entry:b", "beta"), 950);
        assert_eq!(promoted.metadata[JUDGED_BY_KEY], "jev-1.14.0");
        assert!(DoubtVerdicts::was_promoted(&promoted));
    }

    #[test]
    fn thresholds_that_let_one_passage_be_both_are_refused() {
        let new = |veto, promote| {
            DoubtVerdicts::new(
                DoubtEntry::Partial,
                "m".into(),
                "t.v1".into(),
                veto,
                promote,
            )
        };
        assert!(
            new(700, Some(300)).is_err(),
            "0.3 answering is also 0.7 not"
        );
        assert!(new(800, Some(300)).is_ok());
        assert!(new(500, Some(500)).is_err());
        assert!(new(0, None).is_err());
        assert!(new(1001, None).is_err());
        assert!(new(600, Some(401)).is_ok());
        let mut verdicts = new(600, None).expect("valid");
        assert!(
            verdicts
                .judge(
                    "a",
                    "short",
                    DoubtJudgement {
                        answers: 1,
                        not_answers: 1
                    }
                )
                .is_err()
        );
        assert!(
            verdicts
                .judge(
                    "a",
                    &sha("x"),
                    DoubtJudgement {
                        answers: 1001,
                        not_answers: 0
                    }
                )
                .is_err()
        );
    }
}
