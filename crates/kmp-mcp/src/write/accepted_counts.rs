/// How much of the batch the kernel accepted, by section.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AcceptedCounts {
    pub(crate) entries: usize,
    pub(crate) relations: usize,
    pub(crate) evidence: usize,
}
