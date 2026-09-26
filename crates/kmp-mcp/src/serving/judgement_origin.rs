/// Where one judgement's answers came from, as telemetry reports it: the
/// provider over the network, a recorded cassette entry, a cassette that
/// did not hold the request (replay fails; record asks the provider), or the
/// verdict book (every question answered by verdicts already judged).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum JudgementOrigin {
    Remote,
    CassetteHit,
    CassetteMiss,
    BookHit,
}

impl JudgementOrigin {
    /// The stable word a telemetry line carries.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Remote => "remote",
            Self::CassetteHit => "cassette_hit",
            Self::CassetteMiss => "cassette_miss",
            Self::BookHit => "book_hit",
        }
    }

    /// HTTP requests this process actually sent for a judgement that cost
    /// `requests` provider calls: a cassette or book hit sends none.
    pub(crate) fn http_requests(self, requests: usize) -> usize {
        match self {
            Self::CassetteHit | Self::BookHit => 0,
            Self::Remote | Self::CassetteMiss => requests,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::JudgementOrigin;

    #[test]
    fn words_are_stable_and_hits_send_nothing() {
        assert_eq!(JudgementOrigin::Remote.as_str(), "remote");
        assert_eq!(JudgementOrigin::CassetteHit.as_str(), "cassette_hit");
        assert_eq!(JudgementOrigin::CassetteMiss.as_str(), "cassette_miss");
        assert_eq!(JudgementOrigin::BookHit.as_str(), "book_hit");
        assert_eq!(JudgementOrigin::BookHit.http_requests(4), 0);
        assert_eq!(JudgementOrigin::CassetteHit.http_requests(3), 0);
        assert_eq!(JudgementOrigin::CassetteMiss.http_requests(3), 3);
        assert_eq!(JudgementOrigin::Remote.http_requests(2), 2);
    }
}
