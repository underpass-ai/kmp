/// Operational severity for one lifecycle diagnosis.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticSeverity {
    Ok,
    Warn,
    Fail,
}
