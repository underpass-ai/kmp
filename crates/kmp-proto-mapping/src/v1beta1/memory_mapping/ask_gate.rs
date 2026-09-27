use super::confidence_calibration::ConfidenceCalibration;

/// The anchored decision gate of `kmp_ask`, on by default and opted out of
/// per store.
///
/// Off, an ask answers as it did in v0.23.0: the ⌈2/3⌉ rule decides, and the
/// response carries no `answer_status` or `unknown_reason`. On, a question
/// that names an identifier is answered only from memories that name it, and
/// every response says how it settled and, when it did not answer, why
/// (`QuestionContract`, `AnchoredGate`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AskGate {
    partial: bool,
    successor_core: bool,
    calibration: Option<&'static ConfidenceCalibration>,
}

impl AskGate {
    /// The gate of a store that says nothing about it (no `ask-gate.json`).
    ///
    /// The one place that decides whether the anchored gate is the default.
    /// It is: every store reads with the gate, PARTIAL allowed, and a store
    /// opts out with `{"mode":"off"}`, which answers byte for byte as
    /// v0.23.0 did. Made the default on 2026-09-26, after B-real was judged.
    pub const STORE_DEFAULT: Option<AskGate> = Some(AskGate::anchored(true));

    /// The anchored gate. With `partial`, an enumerative question whose
    /// cited memories state only some of what it asked is PARTIAL; without
    /// it, UNKNOWN like a singular one.
    pub const fn anchored(partial: bool) -> Self {
        Self {
            partial,
            successor_core: false,
            calibration: None,
        }
    }

    /// The measured variant of the lifecycle rescue (P7): the current head
    /// of a replaced memory that named the question's principal anchor may
    /// be cited for that anchor, marked `anchor_via: <relation>`, when the
    /// question asks about now. Off unless a store turns it on; the kernel
    /// proposes a successor, and only a store that chose to may cite it.
    pub const fn with_successor_core(mut self, on: bool) -> Self {
        self.successor_core = on;
        self
    }

    /// States `proof.confidence` through a calibration table (P16): a
    /// `high` the table's rules do not stand behind is stated as `medium`.
    /// Off unless a store turns it on.
    pub const fn with_confidence_calibration(
        mut self,
        calibration: Option<&'static ConfidenceCalibration>,
    ) -> Self {
        self.calibration = calibration;
        self
    }

    pub fn confidence_calibration(&self) -> Option<&'static ConfidenceCalibration> {
        self.calibration
    }

    pub fn allows_partial(&self) -> bool {
        self.partial
    }

    pub fn admits_successor_to_core(&self) -> bool {
        self.successor_core
    }
}
