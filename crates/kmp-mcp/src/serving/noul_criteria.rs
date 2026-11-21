/// What a yes and a no mean for one noul, as TypeSafe's `criteria` reads
/// them: the provider is told which reading each answer stands for, so a
/// passage on the same topic is not taken for one that states the fact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NoulCriteria {
    pub when_true: String,
    pub when_false: String,
}

impl NoulCriteria {
    pub(crate) fn new(when_true: &str, when_false: &str) -> Self {
        Self {
            when_true: when_true.to_string(),
            when_false: when_false.to_string(),
        }
    }
}
