/// Estimates token counts for text content.
///
/// Implementations may range from simple heuristics (chars / 4) to
/// model-specific tokenizers. The kernel uses this trait for budget
/// enforcement during context rendering.
pub trait TokenEstimator: Send + Sync {
    fn estimate_tokens(&self, text: &str) -> u32;

    /// Estimate a sequence of records without requiring callers to join it
    /// into one large temporary string. The default preserves the exact
    /// concatenated-text semantics required by arbitrary estimators.
    fn estimate_token_records(&self, records: &mut dyn Iterator<Item = String>) -> u32 {
        let mut text = String::new();
        for record in records {
            text.push_str(&record);
        }
        self.estimate_tokens(&text)
    }

    fn name(&self) -> &str;
}
