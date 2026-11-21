//! Every word `kmp_curate` sends to Jev, in one place: what each relation
//! means, the questions, and what their yes and no mean. Changing a meaning
//! here bumps the template version of the sites that ask it
//! (`JudgementSite::template_version`). Written to TypeSafe's guide
//! (2026-11-21): one judgement per question, a companion yes/no instead of a
//! `none` option, described options, concrete `false` criteria. The
//! comparison with the earlier wording is in
//! `docs/development/jev-evaluation.md`.

/// What each relation a writer may declare means, read from `from` to `to`:
/// the descriptions a typing choice carries for its options.
pub(crate) fn relation_means(relation: &str) -> Option<&'static str> {
    Some(match relation {
        "follows" => "`from` comes after `to` in a process or sequence, with no stronger link.",
        "answers" => "`from` answers a question or request stated in `to`.",
        "uses_background" => "`from` relies on `to` only as background context.",
        "depends_on" => "`from` cannot hold or proceed without `to`.",
        "chosen_because" => "`from` is a decision taken because of what `to` says.",
        "triggers" => "`from` caused or set off the event or action in `to`.",
        "authorizes" => "`from` is an approval that permits the action in `to`.",
        "verified_by" => "the claim or outcome in `from` is confirmed by the check in `to`.",
        "semantic_delta_from" => "`from` describes how the state in `to` changed.",
        "updates_state" => "`from` reports a later state of the same thing described in `to`.",
        "supports" => "`from` gives evidence for `to`.",
        "supersedes" => "`from` replaces `to` entirely; `to` no longer holds.",
        "contradicts" => {
            "`from` and `to` cannot both be true, and neither is a later version of the other."
        }
        "satisfies_constraint" => "`from` complies with the rule or constraint in `to`.",
        "violates_constraint" => "`from` breaks the rule or constraint in `to`.",
        "contributes_to" => "the value in `from` is part of what produced the result in `to`.",
        "excluded_from" => "the item in `from` is left out of the total or set in `to`.",
        "checked_against" => "the value in `from` was checked against the rule or window in `to`.",
        "derived_from" => "`from` is computed or derived from `to`.",
        "confirms_selection" => "`from` is later evidence confirming the choice made in `to`.",
        "restates" => "`from` says the same fact as `to`, in other words or another language.",
        "corrects" => "`from` fixes a specific error in `to` without replacing all of it.",
        "component_of" => "the value in `from` is one part of the larger total or set in `to`.",
        "total_of" => "the value in `from` is the total of component values such as `to`.",
        "same_event_as" => "`from` and `to` describe the same event.",
        "same_entity_as" => "`from` and `to` describe the same entity.",
        "qualifies_as" => "`from` and `to` may describe the same item; one qualifies as the other.",
        "matches_requirement" => "`from` satisfies the requirement or query predicate in `to`.",
        _ => return None,
    })
}

/// A relation as a verb phrase a reader can test in both directions.
pub(crate) fn relation_verb(relation: &str) -> String {
    match relation {
        "supersedes" => "replaces".into(),
        "updates_state" => "reports a later state of what is described in".into(),
        "supports" => "gives evidence for".into(),
        "chosen_because" => "was decided because of".into(),
        "triggers" => "caused".into(),
        "depends_on" => "depends on".into(),
        "verified_by" => "is confirmed by".into(),
        "corrects" => "corrects".into(),
        "derived_from" => "is derived from".into(),
        "satisfies_constraint" => "complies with".into(),
        "violates_constraint" => "breaks".into(),
        "follows" => "comes after".into(),
        "answers" => "answers".into(),
        "contributes_to" => "contributes to".into(),
        "confirms_selection" => "confirms the choice made in".into(),
        other => other.replace('_', " "),
    }
}

/// Pair typing: which relation, beside its companion (is there any?).
pub(crate) const TYPE_Q: &str = "Which relation does `from` have to `to`?";
pub(crate) const RELATED_Q: &str = "Does `from` have a direct relation to `to`?";
pub(crate) const RELATED_TRUE: &str = "one of the two causes, explains, supports, replaces, updates, restates, answers or contradicts the other";
pub(crate) const RELATED_FALSE: &str =
    "the two share a name, number, template or topic, but neither bears on the other";

/// Whether a declared or about-to-be-written relation's own reason holds.
/// Asked without `criteria`: with them, bare causal whys ("X led to Y.")
/// read as unsupported on the held-out set (jev-evaluation.md, 2026-11-21).
pub(crate) const SUPPORT_Q: &str =
    "Do `why` and `evidence` show that `from` has the relation `relation` to `to`?";

/// Which way a relation runs, beside its companion.
pub(crate) const DIRECTION_Q: &str = "Which way does the relation hold?";

/// The companion of a direction choice for `verb`.
pub(crate) fn either_way(verb: &str) -> (String, String, String) {
    (
        format!("Does one of `from` and `to` {verb} the other?"),
        format!("one of the two {verb} the other"),
        format!("neither {verb} the other: they are unrelated, or related in another way"),
    )
}

/// A lifecycle pair: how a new fact stands to a current one of the same
/// identifier and kind, beside its companion (the same specific thing?).
pub(crate) const LIFECYCLE_Q: &str = "How does `new` stand to `old`?";
pub(crate) const LIFECYCLE_OPTIONS: [(&str, &str, &str); 4] = [
    (
        "restates",
        "`new` says the same thing as `old` again, possibly in other words or another language.",
        "restates",
    ),
    (
        "later_state",
        "`new` reports a later state or progress of the same thing `old` describes; `old` was true at its time.",
        "updates_state",
    ),
    (
        "replaces",
        "`new` replaces `old`: what `old` said no longer holds.",
        "supersedes",
    ),
    (
        "conflicts",
        "`new` and `old` cannot both hold, and neither is a later version of the other.",
        "contradicts",
    ),
];
pub(crate) const SAME_Q: &str = "Are `new` and `old` about the same specific thing?";
pub(crate) const SAME_TRUE: &str =
    "the same event, decision, setting or item, possibly at another time or with another value";
pub(crate) const SAME_FALSE: &str =
    "different things that share an identifier, a name, a number or a template";

/// The partner round of a review without focus, for the fact keyed `own`:
/// the choice and its companion.
pub(crate) fn partner(own: &str) -> (String, String, String, String) {
    (
        format!("Which fact in `facts` has the most direct relation to `facts.{own}`?"),
        format!("Does a fact in `facts` have a direct relation to `facts.{own}`?"),
        format!(
            "a fact in `facts` causes, explains, supports, contradicts, answers or repeats `facts.{own}`, or `facts.{own}` does that to it"
        ),
        format!(
            "the other facts only share words, names, numbers or a template with `facts.{own}`"
        ),
    )
}

/// A focused fact `new.<key>` and one shortlisted partner.
pub(crate) fn focus_question(key: &str) -> String {
    format!("Is there a direct relation between `passage` and `new.{key}`?")
}
pub(crate) const FOCUS_TRUE: &str =
    "one of the two causes, explains, supports, contradicts, updates, answers or repeats the other";
pub(crate) const FOCUS_FALSE: &str =
    "the two share a name, number, template or topic, but neither bears on the other";

/// The confirming filter of the partner round (off by default).
pub(crate) fn confirm_question(key: &str) -> String {
    format!("Are `pairs.{key}.a` and `pairs.{key}.b` about the same event, thing or decision?")
}
pub(crate) const CONFIRM_TRUE: &str =
    "one of the two causes, explains, supports, contradicts, answers or repeats the other";
pub(crate) const CONFIRM_FALSE: &str =
    "two notes that follow the same template about different people, teams or things";

/// A path search: does a fact lie on the way, by what the search knows.
pub(crate) const ONWAY_GOAL: (&str, &str, &str) = (
    "Is `passage` a step in what connects `start` to `goal`?",
    "`passage` records an event, decision or state that leads from `start` toward `goal`",
    "`passage` shares a topic or a name with `start` or `goal` but is not on the way between them",
);
pub(crate) const ONWAY_FROM: (&str, &str, &str) = (
    "Is `passage` part of what followed from `start`?",
    "`start` caused `passage`, or caused something that led to it",
    "`passage` came later or shares a topic, but `start` did not lead to it",
);
pub(crate) const ONWAY_FOLLOWED: (&str, &str, &str) = (
    "Did something in `followed` lead to `passage`?",
    "something in `followed` caused `passage` or prompted it",
    "`passage` shares a topic with `followed`, but nothing there led to it",
);

/// A path step from or to the fact keyed `own`: (choice, companion,
/// companion true, companion false).
pub(crate) fn next_step(own: &str) -> (String, String, String, String) {
    (
        format!("Which fact in `facts` is a direct consequence of `facts.{own}`?"),
        format!("Is a fact in `facts` a direct consequence of `facts.{own}`?"),
        format!(
            "a fact in `facts` was caused by `facts.{own}`, decided because of it, or is the next state of the same thing"
        ),
        format!(
            "the other facts only share words, names or a topic with `facts.{own}`; none follows from it"
        ),
    )
}

/// The cause side of [`next_step`].
pub(crate) fn cause_step(own: &str) -> (String, String, String, String) {
    (
        format!("Which fact in `facts` is a direct cause of `facts.{own}`?"),
        format!("Is a fact in `facts` a direct cause of `facts.{own}`?"),
        format!(
            "a fact in `facts` led to `facts.{own}`, is why it was decided, or is the earlier state of the same thing"
        ),
        format!(
            "the other facts only share words, names or a topic with `facts.{own}`; none led to it"
        ),
    )
}

/// Labels keep the question they had: the guide's form (a companion instead
/// of `none`) proposed fewer right values on the held-out set
/// (jev-evaluation.md, 2026-11-21).
pub(crate) fn label_question(key: &str) -> String {
    format!(
        "Which `{key}` does `memory` belong to, judging by the examples of each value in the catalogue? Answer none when none fits."
    )
}
