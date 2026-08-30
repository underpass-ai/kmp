/// What is actually inside a store directory, said in the vocabulary the
/// inventory shows a person.
///
/// The adapter classifies bytes on disk; this type owns the words, so two
/// commands cannot drift into describing the same store differently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreStorage {
    /// The one supported engine (ADR-018).
    Sqlite,
    /// A stamp other than the supported format; the digits when it had any.
    UnsupportedFormat(Option<String>),
    /// A supported stamp over storage files no shipped engine wrote.
    UnsupportedStorage,
}

impl StoreStorage {
    pub fn label(&self) -> String {
        match self {
            Self::Sqlite => "sqlite".to_string(),
            Self::UnsupportedFormat(Some(format)) => {
                format!("unsupported format-{format} artifact")
            }
            Self::UnsupportedFormat(None) => "unsupported format artifact".to_string(),
            Self::UnsupportedStorage => "unsupported storage artifact".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::StoreStorage;

    #[test]
    fn every_storage_kind_labels_itself_with_the_words_info_always_used() {
        assert_eq!(StoreStorage::Sqlite.label(), "sqlite");
        assert_eq!(
            StoreStorage::UnsupportedFormat(Some("1".to_string())).label(),
            "unsupported format-1 artifact"
        );
        assert_eq!(
            StoreStorage::UnsupportedFormat(None).label(),
            "unsupported format artifact"
        );
        assert_eq!(
            StoreStorage::UnsupportedStorage.label(),
            "unsupported storage artifact"
        );
    }
}
