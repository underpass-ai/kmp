/// Who put two facts side by side: the kernel, from signals a reader can
/// check, Jev, from meaning alone, or the write-time lifecycle rule, from a
/// shared principal anchor and entry kind (DESIGN L4 4e).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PairOrigin {
    Kernel {
        signals: Vec<String>,
        why: String,
    },
    Jev,
    /// A new fact and a current one of its about that name the same
    /// principal anchor and share an entry kind. Sharing an anchor is not a
    /// replacement (MemStrata 2606.26511): it is only proposed.
    Lifecycle {
        anchor: String,
        kind: String,
    },
}

impl PairOrigin {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Kernel { .. } => "kernel",
            Self::Jev => "jev",
            Self::Lifecycle { .. } => "lifecycle",
        }
    }
}
