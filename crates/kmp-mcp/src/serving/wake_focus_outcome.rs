use kmp_proto_mapping::v1beta1::JudgedSelection;

/// One frozen focus for a wake, including a declared fallback, so every page
/// of that wake keeps and withholds the same evidence.
#[derive(Clone)]
pub(crate) struct WakeFocusOutcome {
    pub selection: Option<JudgedSelection>,
    pub warning: Option<String>,
}
