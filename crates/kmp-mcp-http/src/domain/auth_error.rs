#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthError {
    Unauthorized(String),
    Unavailable(String),
}
