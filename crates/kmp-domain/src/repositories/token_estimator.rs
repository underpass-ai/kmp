/// Estimates token counts for text content.
///
/// Implementations may range from simple heuristics (chars / 4) to
/// model-specific tokenizers. The kernel uses this trait for budget
/// enforcement during context rendering.
pub trait TokenEstimator: Send + Sync {
    fn estimate_tokens(&self, text: &str) -> u32;

    fn name(&self) -> &str;
}
