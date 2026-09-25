/// Whether a judgement cassette answers from its file or records into it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CassetteMode {
    Replay,
    Record,
}

impl CassetteMode {
    pub(super) fn read(value: Option<&str>) -> Result<Self, String> {
        match value.unwrap_or("replay") {
            "replay" => Ok(Self::Replay),
            "record" => Ok(Self::Record),
            _ => Err("KMP_TYPESAFE_CASSETTE_MODE must be replay or record".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_is_the_default_and_anything_else_is_refused() {
        assert_eq!(CassetteMode::read(None), Ok(CassetteMode::Replay));
        assert_eq!(CassetteMode::read(Some("record")), Ok(CassetteMode::Record));
        assert!(CassetteMode::read(Some("live")).is_err());
    }
}
