use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct DiscoveryDocumentDto {
    pub issuer: String,
    pub jwks_uri: String,
}
