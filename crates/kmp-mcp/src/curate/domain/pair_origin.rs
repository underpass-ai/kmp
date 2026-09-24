/// Who put two facts side by side: the kernel, from signals a reader can
/// check, or Jev, from meaning alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PairOrigin {
    Kernel { signals: Vec<String>, why: String },
    Jev,
}

impl PairOrigin {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Kernel { .. } => "kernel",
            Self::Jev => "jev",
        }
    }
}
