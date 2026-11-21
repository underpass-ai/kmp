use std::collections::BTreeMap;

use serde_json::Value;

use super::noul_criteria::NoulCriteria;

/// One typed question for a judgement model. `instructions` may be a string
/// or structured JSON that names fields in backticks.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum JudgementQuestion {
    /// Yes/no: the answer is the probability of yes.
    Noul { instructions: Value },
    /// Yes/no with what each answer means (TypeSafe noul `criteria`): the
    /// answer is the probability of yes. Its own variant, so a plain noul's
    /// request body, and every key recorded from it, stays what it was.
    CriteriaNoul {
        instructions: Value,
        criteria: NoulCriteria,
    },
    /// One option of a closed set: the answer is the choice plus the
    /// distribution over every option offered. `described` maps an option
    /// to what it means (TypeSafe choice `criteria`); an option it does not
    /// name goes out with no description, so a choice with none described
    /// sends, and is recorded under, exactly what it always did.
    Choice {
        instructions: Value,
        options: Vec<String>,
        described: BTreeMap<String, String>,
    },
    /// One grade of an ordered scale, best first: the answer is the expected
    /// grade (0 is the first level) plus the distribution over every level.
    Score {
        instructions: Value,
        levels: Vec<String>,
    },
}

impl JudgementQuestion {
    /// A choice whose options carry no description.
    pub(crate) fn choice(instructions: Value, options: Vec<String>) -> Self {
        Self::Choice {
            instructions,
            options,
            described: BTreeMap::new(),
        }
    }

    /// A choice whose options each carry what they mean, in code order.
    pub(crate) fn described_choice(instructions: Value, options: Vec<(String, String)>) -> Self {
        Self::Choice {
            instructions,
            options: options.iter().map(|(option, _)| option.clone()).collect(),
            described: options.into_iter().collect(),
        }
    }
}
