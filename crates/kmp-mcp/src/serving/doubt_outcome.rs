use kmp_proto_mapping::v1beta1::DoubtVerdicts;

/// What a doubt band's judge returned for one ask: its verdicts, or the
/// warning that says why the deterministic answer stands alone.
pub(crate) struct DoubtOutcome {
    pub verdicts: Option<DoubtVerdicts>,
    pub warning: Option<String>,
}
