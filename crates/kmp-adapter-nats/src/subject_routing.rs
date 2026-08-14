use super::error::NatsConsumerError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectionSubject {
    GraphNode,
    GraphRelation,
    NodeDetail,
}

impl ProjectionSubject {
    pub(crate) fn parse(subject_prefix: &str, subject: &str) -> Result<Self, NatsConsumerError> {
        let normalized = normalize_subject(subject_prefix, subject)?;
        match normalized.as_str() {
            "graph.node.materialized" => Ok(Self::GraphNode),
            "graph.relation.materialized" => Ok(Self::GraphRelation),
            "node.detail.materialized" => Ok(Self::NodeDetail),
            _ => Err(NatsConsumerError::UnsupportedSubject(normalized)),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::GraphNode => "graph.node.materialized",
            Self::GraphRelation => "graph.relation.materialized",
            Self::NodeDetail => "node.detail.materialized",
        }
    }
}

pub(crate) fn stream_name(subject_prefix: &str, consumer_name: &str) -> String {
    if subject_prefix.is_empty() {
        format!("{consumer_name}.events")
    } else {
        format!("{subject_prefix}.events")
    }
}

pub(crate) fn subject_prefix_pattern(subject_prefix: &str) -> String {
    if subject_prefix.is_empty() {
        String::new()
    } else {
        format!("{subject_prefix}.")
    }
}

fn normalize_subject(subject_prefix: &str, subject: &str) -> Result<String, NatsConsumerError> {
    let trimmed = subject.trim();
    if trimmed.is_empty() {
        return Err(NatsConsumerError::UnsupportedSubject(
            "subject cannot be empty".to_string(),
        ));
    }

    if subject_prefix.is_empty() {
        return Ok(trimmed.to_string());
    }

    let prefix = format!("{subject_prefix}.");
    Ok(trimmed
        .strip_prefix(prefix.as_str())
        .unwrap_or(trimmed)
        .to_string())
}
