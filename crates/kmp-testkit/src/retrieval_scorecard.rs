use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

/// What one judged question produced, and how well.
///
/// Retrieval is an empirical field and this repository could not make an
/// empirical claim about its own retriever: every change to ranking was
/// justified by building a store by hand and showing that a mechanism fires.
/// That is evidence a thing works and no evidence about how much. These are
/// the numbers that turn one into the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrievalOutcome {
    /// The refs a reader judged as carrying the answer.
    pub judged: BTreeSet<String>,
    /// What the response returned, in the order it returned it.
    pub retrieved: Vec<String>,
    /// What the answer actually cited, which is a stricter question than
    /// whether the evidence came back at all.
    pub cited: BTreeSet<String>,
    pub unknown: bool,
    pub used_bytes: u64,
    pub elapsed_millis: u64,
}

impl RetrievalOutcome {
    /// Whether every required passage reached the reader within the cutoff.
    /// This is evidence coverage, not a judgment that an answer is true.
    pub fn has_complete_support_at(&self, k: usize) -> bool {
        !self.judged.is_empty()
            && self
                .judged
                .iter()
                .all(|required| self.retrieved.iter().take(k).any(|item| item == required))
    }

    /// The share of judged refs that appear in the first `k` returned.
    pub fn recall_at(&self, k: usize) -> f64 {
        if self.judged.is_empty() {
            return 0.0;
        }
        let seen = self
            .retrieved
            .iter()
            .take(k)
            .filter(|item| self.judged.contains(*item))
            .collect::<BTreeSet<_>>()
            .len();
        seen as f64 / self.judged.len() as f64
    }

    /// One over the rank of the first judged ref, or zero when none came back.
    pub fn reciprocal_rank(&self) -> f64 {
        self.retrieved
            .iter()
            .position(|item| self.judged.contains(item))
            .map_or(0.0, |index| 1.0 / (index + 1) as f64)
    }

    /// Normalized discounted cumulative gain over binary relevance.
    ///
    /// Recall says whether the answer came back; nDCG says whether it came
    /// back near the top, which is what a reader with a token budget actually
    /// receives.
    pub fn ndcg_at(&self, k: usize) -> f64 {
        if self.judged.is_empty() {
            return 0.0;
        }
        let gain = |index: usize| 1.0 / ((index + 2) as f64).log2();
        let earned = self
            .retrieved
            .iter()
            .take(k)
            .enumerate()
            .filter(|(_, item)| self.judged.contains(*item))
            .map(|(index, _)| gain(index))
            .sum::<f64>();
        let ideal = (0..self.judged.len().min(k)).map(gain).sum::<f64>();
        if earned <= 0.0 || ideal <= 0.0 {
            return 0.0;
        }
        earned / ideal
    }

    /// Whether the answer cited what it was supposed to, rather than merely
    /// retrieving it into the proof.
    pub fn answer_cites_judged(&self) -> bool {
        self.cited.iter().any(|item| self.judged.contains(item))
    }

    /// The question this repository could not answer: how often memory says it
    /// does not know something it demonstrably holds.
    ///
    /// A case is only counted when a reader judged a ref for it, so the answer
    /// is present by construction and UNKNOWN can only be a retrieval failure.
    pub fn is_false_unknown(&self) -> bool {
        self.unknown && !self.judged.is_empty()
    }
}

/// The aggregate over a judged collection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetrievalScorecard {
    pub cases: usize,
    pub recall_at_1: f64,
    pub recall_at_5: f64,
    pub recall_at_10: f64,
    pub mean_reciprocal_rank: f64,
    pub ndcg_at_10: f64,
    pub answer_core_precision: f64,
    pub false_unknown_rate: f64,
    pub mean_used_bytes: f64,
    pub mean_elapsed_millis: f64,
}

impl RetrievalScorecard {
    pub fn score(outcomes: &[RetrievalOutcome]) -> Self {
        let cases = outcomes.len();
        let mean = |value: &dyn Fn(&RetrievalOutcome) -> f64| {
            if cases == 0 {
                0.0
            } else {
                outcomes.iter().map(value).sum::<f64>() / cases as f64
            }
        };
        Self {
            cases,
            recall_at_1: mean(&|outcome| outcome.recall_at(1)),
            recall_at_5: mean(&|outcome| outcome.recall_at(5)),
            recall_at_10: mean(&|outcome| outcome.recall_at(10)),
            mean_reciprocal_rank: mean(&RetrievalOutcome::reciprocal_rank),
            ndcg_at_10: mean(&|outcome| outcome.ndcg_at(10)),
            answer_core_precision: mean(&|outcome| f64::from(outcome.answer_cites_judged())),
            false_unknown_rate: mean(&|outcome| f64::from(outcome.is_false_unknown())),
            mean_used_bytes: mean(&|outcome| outcome.used_bytes as f64),
            mean_elapsed_millis: mean(&|outcome| outcome.elapsed_millis as f64),
        }
    }

    /// The quality columns, in the order the recorded baseline stores them.
    ///
    /// Cost is deliberately not here: a floor that rose because a response got
    /// bigger would be a gate rewarding waste.
    pub fn quality_columns(&self) -> [(&'static str, f64); 6] {
        [
            ("recall_at_1", self.recall_at_1),
            ("recall_at_5", self.recall_at_5),
            ("recall_at_10", self.recall_at_10),
            ("mean_reciprocal_rank", self.mean_reciprocal_rank),
            ("ndcg_at_10", self.ndcg_at_10),
            ("answer_core_precision", self.answer_core_precision),
        ]
    }
}

/// How `kmp_ask` settled a question, read from its response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskVerdict {
    /// An answer, whatever its confidence.
    Answer,
    /// Some facets answered and others named as missing. A negative case
    /// counts it as an abstention only when `missing` names what is absent.
    Partial {
        names_absent: bool,
    },
    Unknown,
}

impl AskVerdict {
    /// How a `kmp_ask` response settled the question.
    ///
    /// A store with the anchored gate says so in `answer_status`; without
    /// it, v0.23.0 answers or says UNKNOWN in `answer`. A PARTIAL is read the
    /// way the benchmark specification scores it, as an abstention only when
    /// `proof.missing` names one of the words the case says are absent.
    pub fn read(answer: &Value, absent: &[String]) -> Self {
        let status = match answer["answer_status"].as_str() {
            Some("unknown") => Some("UNKNOWN"),
            Some("partial") => Some("PARTIAL"),
            Some(_) => Some("ANSWER"),
            None => answer["answer"].as_str(),
        };
        match status {
            Some("UNKNOWN") => Self::Unknown,
            Some("PARTIAL") => {
                let missing = answer["proof"]["missing"].to_string().to_lowercase();
                Self::Partial {
                    names_absent: absent
                        .iter()
                        .any(|word| missing.contains(&word.to_lowercase())),
                }
            }
            _ => Self::Answer,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Answer => "ANSWER",
            Self::Partial { .. } => "PARTIAL",
            Self::Unknown => "UNKNOWN",
        }
    }

    /// Whether the response declined to answer what the store does not hold.
    pub fn abstains(self) -> bool {
        match self {
            Self::Answer => false,
            Self::Partial { names_absent } => names_absent,
            Self::Unknown => true,
        }
    }
}

/// What a guarded case decided: one whose right outcome includes not
/// answering, or not citing, something.
///
/// Retrieval metrics cannot see this. A case with nothing judged scores zero
/// on every one of them whether memory abstained or answered with confidence
/// from the wrong subject, which is exactly the failure an identifier twin
/// (`C6.24` answered from `C6.4`) produces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardedDecision {
    /// The case type, as the judged collection names it.
    pub kind: String,
    /// True when the store holds no answer, so any answer is false.
    pub negative: bool,
    pub verdict: AskVerdict,
    /// Whether the answer cited a ref the case forbids, such as the excluded
    /// anchor of a negated question or the neighbour of an absent one.
    pub cited_forbidden: bool,
    /// Whether the response claimed `high` confidence.
    pub high_confidence: bool,
}

impl GuardedDecision {
    /// An answer where none is stored, or a citation of a forbidden ref.
    pub fn is_false_answer(&self) -> bool {
        (self.negative && !self.verdict.abstains()) || self.cited_forbidden
    }
}

/// False answers over the guarded cases, overall and per case type.
#[derive(Debug, Clone, PartialEq)]
pub struct GuardedScorecard {
    pub cases: usize,
    pub false_answer_rate: f64,
    /// The false answers that also claimed `high` confidence, over all
    /// guarded cases: the ones a caller has no signal to doubt.
    pub high_false_answer_rate: f64,
    /// `(kind, cases, false answers)`, ordered by kind.
    pub by_kind: Vec<(String, usize, usize)>,
}

impl GuardedScorecard {
    pub fn score(decisions: &[GuardedDecision]) -> Self {
        let cases = decisions.len();
        let rate = |count: usize| {
            if cases == 0 {
                0.0
            } else {
                count as f64 / cases as f64
            }
        };
        let mut by_kind = BTreeMap::<&str, (usize, usize)>::new();
        for decision in decisions {
            let slot = by_kind.entry(decision.kind.as_str()).or_default();
            slot.0 += 1;
            slot.1 += usize::from(decision.is_false_answer());
        }
        Self {
            cases,
            false_answer_rate: rate(
                decisions
                    .iter()
                    .filter(|decision| decision.is_false_answer())
                    .count(),
            ),
            high_false_answer_rate: rate(
                decisions
                    .iter()
                    .filter(|decision| decision.is_false_answer() && decision.high_confidence)
                    .count(),
            ),
            by_kind: by_kind
                .into_iter()
                .map(|(kind, (total, false_answers))| (kind.to_string(), total, false_answers))
                .collect(),
        }
    }

    /// The per-kind false-answer rate, for the recorded ceilings.
    pub fn kind_rates(&self) -> Vec<(String, f64)> {
        self.by_kind
            .iter()
            .map(|(kind, total, false_answers)| {
                (kind.clone(), *false_answers as f64 / (*total).max(1) as f64)
            })
            .collect()
    }
}

/// How a recorded baseline row holds the measurement to its number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineBound {
    /// A count: the collection changed if it differs.
    Exact,
    /// Quality that may rise freely.
    Floor,
    /// A failure rate that may fall freely.
    Ceiling,
}

/// One row of the recorded retrieval baseline, measured.
#[derive(Debug, Clone, PartialEq)]
pub struct BaselineRow {
    pub name: String,
    pub value: f64,
    pub bound: BaselineBound,
    /// Beyond the original 35, whose rows predate the guarded cases.
    pub extended: bool,
}

impl BaselineRow {
    fn new(name: impl Into<String>, value: f64, bound: BaselineBound, extended: bool) -> Self {
        Self {
            name: name.into(),
            value,
            bound,
            extended,
        }
    }

    /// The value as the file records it. Floors truncate and ceilings round
    /// up, never to nearest: a bound recorded on the wrong side of the number
    /// it was taken from would fail the build that wrote it.
    fn recorded(&self) -> String {
        match self.bound {
            BaselineBound::Exact => format!("{}", self.value as u64),
            BaselineBound::Floor => format!("{:.4}", (self.value * 10_000.0).floor() / 10_000.0),
            BaselineBound::Ceiling => format!("{:.4}", (self.value * 10_000.0).ceil() / 10_000.0),
        }
    }

    /// Why the measurement breaks the recorded bound, if it does. A hair of
    /// tolerance, so a float one ulp off does not fail a change that moved
    /// nothing.
    fn failure(&self, bound: f64) -> Option<String> {
        let (name, measured) = (&self.name, self.value);
        match self.bound {
            BaselineBound::Exact if (measured - bound).abs() > 1e-9 => Some(format!(
                "the collection changed size: {name} recorded {bound}, {measured} run"
            )),
            BaselineBound::Floor if measured + 1e-9 < bound => {
                Some(format!("{name} fell to {measured:.4}, below {bound:.4}"))
            }
            BaselineBound::Ceiling if measured - 1e-9 > bound => {
                Some(format!("{name} rose to {measured:.4}, above {bound:.4}"))
            }
            _ => None,
        }
    }
}

/// Every recorded row, in file order: the original 35 first, unchanged in
/// name and meaning, then every case with a judged answer and the guarded
/// cases.
pub fn baseline_rows(
    original: &RetrievalScorecard,
    every: &RetrievalScorecard,
    guarded: &GuardedScorecard,
) -> Vec<BaselineRow> {
    use BaselineBound::{Ceiling, Exact, Floor};
    let mut rows = vec![BaselineRow::new(
        "cases",
        original.cases as f64,
        Exact,
        false,
    )];
    for (name, value) in original.quality_columns() {
        rows.push(BaselineRow::new(name, value, Floor, false));
    }
    rows.push(BaselineRow::new(
        "false_unknown_rate",
        original.false_unknown_rate,
        Ceiling,
        false,
    ));
    rows.push(BaselineRow::new(
        "all_positives",
        every.cases as f64,
        Exact,
        true,
    ));
    for (name, value) in every.quality_columns() {
        rows.push(BaselineRow::new(format!("all_{name}"), value, Floor, true));
    }
    rows.push(BaselineRow::new(
        "all_false_unknown_rate",
        every.false_unknown_rate,
        Ceiling,
        true,
    ));
    rows.push(BaselineRow::new(
        "guarded_cases",
        guarded.cases as f64,
        Exact,
        true,
    ));
    rows.push(BaselineRow::new(
        "guarded_false_answer_rate",
        guarded.false_answer_rate,
        Ceiling,
        true,
    ));
    rows.push(BaselineRow::new(
        "guarded_high_false_answer_rate",
        guarded.high_false_answer_rate,
        Ceiling,
        true,
    ));
    for (kind, value) in guarded.kind_rates() {
        rows.push(BaselineRow::new(
            format!("false_answer_rate_{kind}"),
            value,
            Ceiling,
            true,
        ));
    }
    rows
}

/// The baseline file for these rows.
pub fn render_baseline(rows: &[BaselineRow]) -> String {
    let mut out = String::from(
        "# Recorded retrieval quality. A number may rise freely; lowering one is a\n\
         # reviewed change that says why. Cost is reported by the scorecard and\n\
         # deliberately not recorded here — a floor that rose because responses grew\n\
         # would be a gate rewarding waste.\n\
         #\n\
         # The unprefixed rows score the original 35 judged cases. The all_ rows score\n\
         # every case with a judged answer, and the guarded_ and false_answer_rate_\n\
         # rows the cases whose right outcome includes not answering or not citing\n\
         # something. Rows named false_ are ceilings, the inverse of a floor: they may\n\
         # fall freely, and raising one is the reviewed change. Counts must match.\n\
         #\n\
         # Refresh deliberately, never to make a red build green:\n\
         #   RETRIEVAL_BASELINE=write cargo run -p kmp-testkit --bin retrieval_kmp_scorecard\n\
         #\n\
         # Reviewed change (2026-09-26): the anchored ask gate became the default, so\n\
         # these rows are read with it. The 35 original rows do not move. The all_ rows\n\
         # rise and all_false_unknown_rate falls from 0.1429 to 0.0477 (the two\n\
         # anchored enumerative and two negated-anchor cases the rule left UNKNOWN\n\
         # are answered from the memories that name their anchor); every false_answer ceiling of the\n\
         # guarded cases falls to 0 (anchor_absent 0.5 -> 0, anchor_neighbor_existing\n\
         # 0.5 -> 0, guarded_false_answer_rate 0.2308 -> 0, guarded_high 0.0770 -> 0):\n\
         # an identifier no cited memory names is UNKNOWN. Measure the rule of v0.23.0\n\
         # with RETRIEVAL_ASK_GATE=off (reported, not gated).\n\
         metric\tfloor\n",
    );
    for row in rows {
        out.push_str(&format!("{}\t{}\n", row.name, row.recorded()));
    }
    out
}

/// Every way the measured rows break a recorded baseline, empty when it
/// holds. A measured row the file does not name fails too: a new guarded
/// kind has no ceiling until one is recorded deliberately.
pub fn baseline_failures(recorded: &str, rows: &[BaselineRow]) -> Result<Vec<String>, String> {
    let mut failures = Vec::new();
    let mut seen = BTreeSet::new();
    for line in recorded.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("metric\t") {
            continue;
        }
        let (metric, bound) = line
            .split_once('\t')
            .ok_or_else(|| format!("malformed baseline row: {line}"))?;
        let bound: f64 = bound
            .parse()
            .map_err(|_| format!("malformed baseline bound: {line}"))?;
        let row = rows
            .iter()
            .find(|row| row.name == metric)
            .ok_or_else(|| format!("baseline names an unknown metric: {metric}"))?;
        seen.insert(metric);
        failures.extend(row.failure(bound));
    }
    for row in rows.iter().filter(|row| !seen.contains(row.name.as_str())) {
        failures.push(format!(
            "{} ({:.4}) has no recorded bound; record it deliberately",
            row.name, row.value
        ));
    }
    Ok(failures)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decision(
        kind: &str,
        negative: bool,
        verdict: AskVerdict,
        forbidden: bool,
    ) -> GuardedDecision {
        GuardedDecision {
            kind: kind.to_string(),
            negative,
            verdict,
            cited_forbidden: forbidden,
            high_confidence: true,
        }
    }

    #[test]
    fn a_response_is_read_as_unknown_partial_or_answer() {
        let absent = ["Kubernetes".to_string()];
        let unknown = serde_json::json!({"answer": "UNKNOWN"});
        let partial =
            serde_json::json!({"answer": "PARTIAL", "proof": {"missing": ["kubernetes cluster"]}});
        let vague = serde_json::json!({"answer": "PARTIAL", "proof": {"missing": ["deployment"]}});
        let answered = serde_json::json!({"answer": "The staging environment."});

        assert_eq!(AskVerdict::read(&unknown, &absent), AskVerdict::Unknown);
        assert_eq!(
            AskVerdict::read(&partial, &absent),
            AskVerdict::Partial { names_absent: true }
        );
        assert_eq!(
            AskVerdict::read(&vague, &absent),
            AskVerdict::Partial {
                names_absent: false
            }
        );
        assert_eq!(AskVerdict::read(&answered, &absent), AskVerdict::Answer);
        // The gate's status speaks first: its PARTIAL keeps the citations in
        // `answer`.
        let gated = |status: &str| {
            serde_json::json!({"answer": "decision:x", "answer_status": status,
                               "proof": {"missing": ["kubernetes"]}})
        };
        assert_eq!(
            AskVerdict::read(&gated("partial"), &absent),
            AskVerdict::Partial { names_absent: true }
        );
        assert_eq!(
            AskVerdict::read(&gated("unknown"), &absent),
            AskVerdict::Unknown
        );
        assert_eq!(
            AskVerdict::read(&gated("answered"), &absent),
            AskVerdict::Answer
        );
        assert_eq!(
            ["ANSWER", "PARTIAL", "UNKNOWN"],
            [
                AskVerdict::Answer.label(),
                AskVerdict::Partial { names_absent: true }.label(),
                AskVerdict::Unknown.label()
            ]
        );
    }

    fn baseline_fixture() -> Vec<BaselineRow> {
        let original = RetrievalScorecard::score(&[
            outcome(&["a"], &["a"], &["a"], false),
            outcome(&["b"], &[], &[], true),
            outcome(&["c"], &["c"], &["c"], false),
        ]);
        let guarded = GuardedScorecard::score(&[
            decision("anchor_absent", true, AskVerdict::Answer, false),
            decision("anchor_absent", true, AskVerdict::Unknown, false),
            decision("near_miss_attribute", true, AskVerdict::Unknown, false),
        ]);
        baseline_rows(&original, &original, &guarded)
    }

    #[test]
    fn the_original_rows_keep_their_names_and_the_rest_are_prefixed() {
        let rows = baseline_fixture();
        let names = rows.iter().map(|row| row.name.as_str()).collect::<Vec<_>>();

        assert_eq!(
            &names[..8],
            &[
                "cases",
                "recall_at_1",
                "recall_at_5",
                "recall_at_10",
                "mean_reciprocal_rank",
                "ndcg_at_10",
                "answer_core_precision",
                "false_unknown_rate",
            ]
        );
        assert!(rows[..8].iter().all(|row| !row.extended));
        assert!(rows[8..].iter().all(|row| row.extended));
        assert!(names.contains(&"false_answer_rate_anchor_absent"));
        assert!(names.contains(&"guarded_high_false_answer_rate"));
    }

    #[test]
    fn floors_truncate_and_ceilings_round_up_so_the_writer_passes_its_own_file() {
        let rows = baseline_fixture();
        let file = render_baseline(&rows);

        assert!(file.contains("\ncases\t3\n"));
        assert!(file.contains("\nrecall_at_1\t0.6666\n"));
        assert!(file.contains("\nfalse_unknown_rate\t0.3334\n"));
        assert!(file.contains("\nguarded_false_answer_rate\t0.3334\n"));
        assert_eq!(baseline_failures(&file, &rows), Ok(Vec::new()));
    }

    #[test]
    fn a_fallen_floor_a_risen_ceiling_and_a_changed_count_all_fail() {
        let rows = baseline_fixture();
        let file = render_baseline(&rows)
            .replace("\ncases\t3\n", "\ncases\t4\n")
            .replace("\nrecall_at_1\t0.6666\n", "\nrecall_at_1\t0.9000\n")
            .replace(
                "\nguarded_false_answer_rate\t0.3334\n",
                "\nguarded_false_answer_rate\t0.0000\n",
            );

        let failures = baseline_failures(&file, &rows).expect("well formed");

        assert_eq!(failures.len(), 3, "{failures:?}");
        assert!(failures[0].contains("changed size"));
        assert!(failures[1].contains("recall_at_1 fell"));
        assert!(failures[2].contains("guarded_false_answer_rate rose"));
    }

    #[test]
    fn an_unrecorded_row_fails_and_a_malformed_file_is_an_error() {
        let rows = baseline_fixture();
        let file =
            render_baseline(&rows).replace("false_answer_rate_near_miss_attribute\t0.0000\n", "");

        let failures = baseline_failures(&file, &rows).expect("well formed");
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("no recorded bound"));

        assert!(baseline_failures("recall_at_1 0.5", &rows).is_err());
        assert!(baseline_failures("recall_at_1\tmany", &rows).is_err());
        assert!(baseline_failures("unknown_metric\t0.5", &rows).is_err());
    }

    #[test]
    fn a_negative_is_answered_falsely_unless_memory_abstains() {
        let answered = decision("anchor_absent", true, AskVerdict::Answer, false);
        let declined = decision("anchor_absent", true, AskVerdict::Unknown, false);
        let named = decision(
            "anchor_absent",
            true,
            AskVerdict::Partial { names_absent: true },
            false,
        );
        let vague = decision(
            "anchor_absent",
            true,
            AskVerdict::Partial {
                names_absent: false,
            },
            false,
        );

        assert!(answered.is_false_answer());
        assert!(!declined.is_false_answer());
        assert!(!named.is_false_answer());
        assert!(vague.is_false_answer());
    }

    #[test]
    fn a_guarded_positive_is_false_only_when_it_cites_what_it_excludes() {
        let clean = decision("negated_anchor", false, AskVerdict::Answer, false);
        let leaked = decision("negated_anchor", false, AskVerdict::Answer, true);

        assert!(!clean.is_false_answer());
        assert!(leaked.is_false_answer());
    }

    #[test]
    fn guarded_scorecard_counts_per_kind_and_separates_high_confidence() {
        let mut quiet = decision("near_miss_attribute", true, AskVerdict::Answer, false);
        quiet.high_confidence = false;
        let card = GuardedScorecard::score(&[
            decision("anchor_absent", true, AskVerdict::Answer, false),
            decision("anchor_absent", true, AskVerdict::Unknown, false),
            quiet,
            decision("negated_anchor", false, AskVerdict::Answer, false),
        ]);

        assert_eq!(card.cases, 4);
        assert_eq!(card.false_answer_rate, 0.5);
        assert_eq!(card.high_false_answer_rate, 0.25);
        assert_eq!(
            card.by_kind,
            vec![
                ("anchor_absent".to_string(), 2, 1),
                ("near_miss_attribute".to_string(), 1, 1),
                ("negated_anchor".to_string(), 1, 0),
            ]
        );
        assert_eq!(
            card.kind_rates(),
            vec![
                ("anchor_absent".to_string(), 0.5),
                ("near_miss_attribute".to_string(), 1.0),
                ("negated_anchor".to_string(), 0.0),
            ]
        );
        assert_eq!(GuardedScorecard::score(&[]).false_answer_rate, 0.0);
    }

    fn outcome(
        judged: &[&str],
        retrieved: &[&str],
        cited: &[&str],
        unknown: bool,
    ) -> RetrievalOutcome {
        RetrievalOutcome {
            judged: judged.iter().map(|item| (*item).to_string()).collect(),
            retrieved: retrieved.iter().map(|item| (*item).to_string()).collect(),
            cited: cited.iter().map(|item| (*item).to_string()).collect(),
            unknown,
            used_bytes: 0,
            elapsed_millis: 0,
        }
    }

    #[test]
    fn recall_counts_only_what_arrived_inside_the_cutoff() {
        let found = outcome(&["a"], &["x", "y", "a"], &[], false);

        assert_eq!(found.recall_at(1), 0.0);
        assert_eq!(found.recall_at(5), 1.0);
    }

    #[test]
    fn recall_is_a_share_when_more_than_one_ref_was_judged() {
        let half = outcome(&["a", "b"], &["a", "x"], &[], false);

        assert_eq!(half.recall_at(5), 0.5);
    }

    #[test]
    fn complete_support_needs_every_distinct_passage_inside_the_cutoff() {
        let partial = outcome(
            &["claim", "alias"],
            &["claim", "claim", "alias"],
            &[],
            false,
        );
        assert!(!partial.has_complete_support_at(2));
        assert!(partial.has_complete_support_at(3));
        assert!(!outcome(&[], &[], &[], false).has_complete_support_at(10));
    }

    #[test]
    fn rank_is_rewarded_and_absence_scores_nothing() {
        let first = outcome(&["a"], &["a", "x"], &[], false);
        let third = outcome(&["a"], &["x", "y", "a"], &[], false);
        let missing = outcome(&["a"], &["x", "y"], &[], false);

        assert_eq!(first.reciprocal_rank(), 1.0);
        assert!((third.reciprocal_rank() - 1.0 / 3.0).abs() < 1e-12);
        assert_eq!(missing.reciprocal_rank(), 0.0);
        assert!(first.ndcg_at(10) > third.ndcg_at(10));
        assert_eq!(missing.ndcg_at(10), 0.0);
        assert_eq!(first.ndcg_at(10), 1.0);
    }

    /// Retrieving the answer into the proof and citing it are different
    /// claims, and the graph traversal makes the difference deliberate.
    #[test]
    fn citing_is_stricter_than_retrieving() {
        let reached = outcome(&["a"], &["a"], &[], false);
        let cited = outcome(&["a"], &["a"], &["a"], false);

        assert_eq!(reached.recall_at(5), 1.0);
        assert!(!reached.answer_cites_judged());
        assert!(cited.answer_cites_judged());
    }

    #[test]
    fn unknown_is_false_only_when_the_answer_was_judged_to_exist() {
        let withheld = outcome(&["a"], &[], &[], true);
        let honestly_absent = outcome(&[], &[], &[], true);

        assert!(withheld.is_false_unknown());
        assert!(!honestly_absent.is_false_unknown());
    }

    #[test]
    fn a_scorecard_averages_over_the_collection() {
        let card = RetrievalScorecard::score(&[
            outcome(&["a"], &["a"], &["a"], false),
            outcome(&["b"], &["x"], &[], true),
        ]);

        assert_eq!(card.cases, 2);
        assert_eq!(card.recall_at_5, 0.5);
        assert_eq!(card.answer_core_precision, 0.5);
        assert_eq!(card.false_unknown_rate, 0.5);
        assert_eq!(card.quality_columns().len(), 6);
    }

    #[test]
    fn an_empty_collection_scores_zero_rather_than_dividing_by_it() {
        let card = RetrievalScorecard::score(&[]);

        assert_eq!(card.cases, 0);
        assert_eq!(card.recall_at_5, 0.0);
        assert_eq!(card.ndcg_at_10, 0.0);
    }
}
