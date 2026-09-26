/// The anchored decision gate of `kmp_ask`, opted into per store.
///
/// Off, an ask answers exactly as it did: the ⌈2/3⌉ rule decides, and the
/// response carries no `answer_status` or `unknown_reason`. On, a question
/// that names an identifier is answered only from memories that name it, and
/// every response says how it settled and, when it did not answer, why
/// (`QuestionContract`, `AnchoredGate`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AskGate {
    partial: bool,
}

impl AskGate {
    /// The anchored gate. With `partial`, an enumerative question whose
    /// cited memories state only some of what it asked is PARTIAL; without
    /// it, UNKNOWN like a singular one.
    pub fn anchored(partial: bool) -> Self {
        Self { partial }
    }

    pub fn allows_partial(&self) -> bool {
        self.partial
    }
}
