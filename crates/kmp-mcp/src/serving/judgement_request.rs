use std::collections::BTreeMap;

use serde_json::Value;

use super::judgement_question::JudgementQuestion;

/// One state judged by named questions. Keys are chosen by the caller and
/// come back unchanged; they are not shown to the model.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JudgementRequest {
    pub state: Value,
    pub questions: BTreeMap<String, JudgementQuestion>,
}
