/// What a view intent asked for that this store or session cannot show.
#[derive(Debug, Default)]
pub(crate) struct UnhonoredProjection {
    pub(crate) dimensions: Vec<String>,
    pub(crate) overlays: Vec<String>,
}
