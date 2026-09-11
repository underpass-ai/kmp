/// An absent required membership at a specific role and stored ref.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EvidenceMissingWitness {
    pub role: String,
    pub at: u32,
    pub reference: String,
    pub key: String,
    pub name: String,
}
