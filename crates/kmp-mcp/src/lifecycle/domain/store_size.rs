/// Bytes a store occupies on disk, which knows how to say itself to a person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreSize(u64);

impl StoreSize {
    pub fn new(bytes: u64) -> Self {
        Self(bytes)
    }

    /// A size a person reads at a glance.
    pub fn human(self) -> String {
        if self.0 >= 1_048_576 {
            format!("{:.1}M", self.0 as f64 / 1_048_576.0)
        } else if self.0 >= 1_024 {
            format!("{}K", self.0 / 1_024)
        } else {
            format!("{}B", self.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::StoreSize;

    #[test]
    fn a_size_reads_in_the_unit_a_person_would_pick() {
        assert_eq!(StoreSize::new(443).human(), "443B");
        assert_eq!(StoreSize::new(1_024).human(), "1K");
        assert_eq!(StoreSize::new(1_468_006).human(), "1.4M");
    }
}
