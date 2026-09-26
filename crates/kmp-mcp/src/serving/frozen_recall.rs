use kmp_proto::v1beta1::{AskResponse, WakeResponse};
use prost::Message;
use serde_json::Value;

/// One recall read as it stood before its page was cut: the kernel's
/// selection, lifecycle and proof, with the remote channels' verdicts
/// already applied, and its render, the JSON every page is cut from.
/// Cutting any page of it again is the projection alone.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FrozenRecall {
    Wake {
        response: Box<WakeResponse>,
        rendered: Value,
    },
    Ask {
        response: Box<AskResponse>,
        rendered: Value,
    },
}

impl FrozenRecall {
    /// An estimate of what the read holds in memory, which bounds what the
    /// process keeps: the encoded response, and twice the serialized render
    /// for the tree that holds it.
    pub(crate) fn approximate_bytes(&self) -> usize {
        let (encoded, rendered) = match self {
            Self::Wake { response, rendered } => (response.encoded_len(), rendered),
            Self::Ask { response, rendered } => (response.encoded_len(), rendered),
        };
        let serialized = serde_json::to_vec(rendered).map_or(0, |bytes| bytes.len());
        encoded + 2 * serialized
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_size_grows_with_the_read_and_its_render() {
        let small = FrozenRecall::Ask {
            response: Box::default(),
            rendered: Value::Null,
        };
        let large = FrozenRecall::Ask {
            response: Box::new(AskResponse {
                summary: "x".repeat(4096),
                ..AskResponse::default()
            }),
            rendered: json!({"summary": "x".repeat(4096)}),
        };
        assert!(large.approximate_bytes() >= 3 * 4096);
        assert!(small.approximate_bytes() < large.approximate_bytes());
        assert_eq!(
            FrozenRecall::Wake {
                response: Box::default(),
                rendered: json!({}),
            }
            .approximate_bytes(),
            4
        );
    }
}
