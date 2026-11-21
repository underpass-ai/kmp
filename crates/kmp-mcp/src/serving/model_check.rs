/// Whether the model a store pins is still served by TypeSafe, as `doctor`
/// asks it (`GET /v1/models`, no store text sent): a deprecated pin shows up
/// before every judgement fails.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ModelCheck {
    /// The pinned model is in TypeSafe's list.
    Served { model: String },
    /// TypeSafe answered, and the pinned model is not in its list.
    NotServed { model: String, served: Vec<String> },
    /// No answer to read: no key, the service unreachable, a refusal. Never
    /// a verdict on the pin.
    NotChecked { model: String, reason: String },
}
