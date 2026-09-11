/// Compact union of entry identities. Its universe belongs to one candidate set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TraceMaterialSet(Vec<u64>);

impl TraceMaterialSet {
    pub fn empty(nodes: usize) -> Self {
        Self(vec![0; nodes.div_ceil(64)])
    }
    pub fn insert(&mut self, index: usize) {
        self.0[index / 64] |= 1 << (index % 64);
    }
    pub fn union(&mut self, other: &Self) {
        for (a, b) in self.0.iter_mut().zip(&other.0) {
            *a |= b;
        }
    }
    pub fn count(&self) -> u32 {
        self.0.iter().map(|w| w.count_ones()).sum()
    }
    pub fn intersection_count(&self, other: &Self) -> u32 {
        self.0
            .iter()
            .zip(&other.0)
            .map(|(a, b)| (a & b).count_ones())
            .sum()
    }
    pub fn contains(&self, index: usize) -> bool {
        self.0[index / 64] & (1 << (index % 64)) != 0
    }
}
