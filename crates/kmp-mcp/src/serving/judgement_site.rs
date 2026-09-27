use std::time::Duration;

use super::verdict_template::VerdictTemplate;

/// The caller a judgement is spent for, so its cost can be attributed. The
/// words are a telemetry contract read by the memory bench
/// (`scripts/performance/memory_bench/SCHEMAS.md`); add, never rename.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum JudgementSite {
    /// `kmp_ask` re-ranking (`rerank.json`).
    Rerank,
    /// `kmp_wake` focused on its intent (`wake-focus.json`).
    WakeFocus,
    /// `kmp_curate` review without focus.
    CurateReview,
    /// `kmp_curate` review of focused refs.
    CurateFocus,
    /// Relations proposed after a write (`write-relations.json`).
    WriteRelations,
    /// The pre-check of relations an agent accepted with `mode: apply`.
    Precheck,
    /// `kmp_curate` path search.
    Paths,
    /// `kmp_curate` label proposals.
    Labels,
    /// `kmp_summaries_audit` meaning check.
    Summaries,
    /// `kmp_ask` in the doubt band (`ask-judge.json`).
    DoubtBand,
    /// Search expansions proposed at write (`write-expansions.json`).
    Expansions,
}

impl JudgementSite {
    /// Every site, for what must cover them all (the verdict book retiring
    /// superseded templates).
    pub(crate) const ALL: [Self; 11] = [
        Self::Rerank,
        Self::WakeFocus,
        Self::CurateReview,
        Self::CurateFocus,
        Self::WriteRelations,
        Self::Precheck,
        Self::Paths,
        Self::Labels,
        Self::Summaries,
        Self::DoubtBand,
        Self::Expansions,
    ];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Rerank => "rerank",
            Self::WakeFocus => "wake_focus",
            Self::CurateReview => "curate_review",
            Self::CurateFocus => "curate_focus",
            Self::WriteRelations => "write_relations",
            Self::Precheck => "precheck",
            Self::Paths => "paths",
            Self::Labels => "labels",
            Self::Summaries => "summaries",
            Self::DoubtBand => "doubt_band",
            Self::Expansions => "expansions",
        }
    }

    /// The prompt template this site's verdicts answer. Bump a version when
    /// what the site asks changes meaning without its text changing, so the
    /// verdict book stops answering it with the old meaning.
    pub(crate) fn template(self) -> VerdictTemplate {
        VerdictTemplate {
            id: self.as_str(),
            version: 1,
        }
    }

    /// How long a first page may wait for Jev before it degrades, warned,
    /// to the deterministic path (DESIGN L4 4b): 1.5 s for ask re-ranking
    /// and the ask doubt band, 3 s for the first page of a focused wake,
    /// none for curate, whose agent asked for the judgement itself.
    pub(crate) fn deadline(self) -> Option<Duration> {
        match self {
            Self::Rerank | Self::DoubtBand => Some(Duration::from_millis(1_500)),
            Self::WakeFocus => Some(Duration::from_millis(3_000)),
            _ => None,
        }
    }

    /// The site of one `kmp_curate` call, read from the arguments the
    /// backend dispatches on. `write_proposals` is the write dispatcher's
    /// internal review.
    pub(crate) fn of_curate(arguments: &serde_json::Value) -> Self {
        let focused = arguments
            .get("focus")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|refs| !refs.is_empty());
        match arguments.get("mode").and_then(serde_json::Value::as_str) {
            Some("write_proposals") => Self::WriteRelations,
            Some("prepare_apply") => Self::Precheck,
            Some("paths") => Self::Paths,
            Some("labels") => Self::Labels,
            Some("judge_expansions") => Self::Expansions,
            _ if focused => Self::CurateFocus,
            _ => Self::CurateReview,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::JudgementSite;

    #[test]
    fn curate_modes_name_their_site() {
        let site = |arguments| JudgementSite::of_curate(&arguments).as_str();
        assert_eq!(site(json!({"mode": "review"})), "curate_review");
        assert_eq!(
            site(json!({"mode": "review", "focus": []})),
            "curate_review"
        );
        assert_eq!(
            site(json!({"mode": "review", "focus": ["a"]})),
            "curate_focus"
        );
        assert_eq!(
            site(json!({"mode": "write_proposals", "focus": ["a"]})),
            "write_relations"
        );
        assert_eq!(site(json!({"mode": "prepare_apply"})), "precheck");
        assert_eq!(site(json!({"mode": "paths"})), "paths");
        assert_eq!(site(json!({"mode": "labels", "focus": ["a"]})), "labels");
        assert_eq!(site(json!({"mode": "judge_expansions"})), "expansions");
    }

    #[test]
    fn only_first_pages_of_reads_carry_a_deadline() {
        assert_eq!(
            JudgementSite::Rerank.deadline(),
            Some(std::time::Duration::from_millis(1_500))
        );
        assert_eq!(
            JudgementSite::WakeFocus.deadline(),
            Some(std::time::Duration::from_millis(3_000))
        );
        assert_eq!(
            JudgementSite::DoubtBand.deadline(),
            Some(std::time::Duration::from_millis(1_500))
        );
        assert_eq!(JudgementSite::DoubtBand.as_str(), "doubt_band");
        assert_eq!(JudgementSite::CurateReview.deadline(), None);
        assert_eq!(JudgementSite::Paths.deadline(), None);
        assert_eq!(JudgementSite::Paths.template().id, "paths");
    }

    #[test]
    fn every_site_has_a_distinct_word() {
        let all = JudgementSite::ALL;
        let words = all
            .iter()
            .map(|site| site.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(words.len(), all.len());
    }
}
