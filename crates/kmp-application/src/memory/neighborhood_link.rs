/// Indices address stored items in this packet, not canonical memory refs.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NeighborhoodLink {
    pub from: usize,
    pub rel: String,
    pub to: usize,
}
