use prost_types::Timestamp;
use rehydration_application::MemoryAnswerPolicy;
use rehydration_domain::{ResolutionTier, TemporalDirection};
use rehydration_proto::v1beta1::{
    AnswerPolicy as ProtoAnswerPolicy, MemoryConfidence, MemoryDetailLevel, MemorySemanticClass,
    MemorySourceKind, TemporalDirection as ProtoTemporalDirection,
};
use tonic::Status;

const UNIX_SORT_OFFSET: i64 = 100_000_000_000;

pub(super) type ProtoMappingResult<T> = Result<T, Box<Status>>;

pub(super) fn invalid_argument(message: impl Into<String>) -> Box<Status> {
    Box::new(Status::invalid_argument(message.into()))
}

pub(super) fn non_empty(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

pub(super) fn plural<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 { singular } else { plural }
}

pub(super) fn memory_detail_level(value: i32) -> ProtoMappingResult<MemoryDetailLevel> {
    MemoryDetailLevel::try_from(value)
        .map_err(|_| invalid_argument("memory budget detail is invalid"))
}

pub(super) fn max_tier_from_detail(value: MemoryDetailLevel) -> Option<ResolutionTier> {
    match value {
        MemoryDetailLevel::Compact => Some(ResolutionTier::L0Summary),
        MemoryDetailLevel::Balanced => Some(ResolutionTier::L1CausalSpine),
        MemoryDetailLevel::Full => Some(ResolutionTier::L2EvidencePack),
        MemoryDetailLevel::Unspecified => None,
    }
}

pub(super) fn answer_policy_from_proto(value: i32) -> ProtoMappingResult<MemoryAnswerPolicy> {
    Ok(
        match ProtoAnswerPolicy::try_from(value)
            .map_err(|_| invalid_argument("answer policy is invalid"))?
        {
            ProtoAnswerPolicy::Unspecified | ProtoAnswerPolicy::EvidenceOrUnknown => {
                MemoryAnswerPolicy::EvidenceOrUnknown
            }
            ProtoAnswerPolicy::ShowConflicts => MemoryAnswerPolicy::ShowConflicts,
            ProtoAnswerPolicy::BestEffort => MemoryAnswerPolicy::BestEffort,
        },
    )
}

pub(super) fn proto_direction(value: TemporalDirection) -> ProtoTemporalDirection {
    match value {
        TemporalDirection::Goto => ProtoTemporalDirection::Goto,
        TemporalDirection::Near => ProtoTemporalDirection::Near,
        TemporalDirection::Rewind => ProtoTemporalDirection::Rewind,
        TemporalDirection::Forward => ProtoTemporalDirection::Forward,
    }
}

pub(super) fn proto_semantic_class(
    value: &rehydration_domain::RelationSemanticClass,
) -> MemorySemanticClass {
    match value {
        rehydration_domain::RelationSemanticClass::Structural => MemorySemanticClass::Structural,
        rehydration_domain::RelationSemanticClass::Causal => MemorySemanticClass::Causal,
        rehydration_domain::RelationSemanticClass::Motivational => {
            MemorySemanticClass::Motivational
        }
        rehydration_domain::RelationSemanticClass::Procedural => MemorySemanticClass::Procedural,
        rehydration_domain::RelationSemanticClass::Evidential => MemorySemanticClass::Evidential,
        rehydration_domain::RelationSemanticClass::Constraint => MemorySemanticClass::Constraint,
    }
}

pub(super) fn proto_confidence(value: Option<&str>) -> MemoryConfidence {
    match value.unwrap_or("").trim().to_ascii_lowercase().as_str() {
        "high" => MemoryConfidence::High,
        "medium" => MemoryConfidence::Medium,
        "low" => MemoryConfidence::Low,
        _ => MemoryConfidence::Unknown,
    }
}

pub(super) fn semantic_class_name(value: MemorySemanticClass) -> String {
    match value {
        MemorySemanticClass::Structural => "structural",
        MemorySemanticClass::Causal => "causal",
        MemorySemanticClass::Motivational => "motivational",
        MemorySemanticClass::Procedural => "procedural",
        MemorySemanticClass::Evidential => "evidential",
        MemorySemanticClass::Constraint => "constraint",
        _ => "",
    }
    .to_string()
}

pub(super) fn confidence_name(value: MemoryConfidence) -> Option<String> {
    let value = match value {
        MemoryConfidence::High => "high",
        MemoryConfidence::Medium => "medium",
        MemoryConfidence::Low => "low",
        MemoryConfidence::Unknown => "unknown",
        _ => "",
    };
    non_empty(value.to_string())
}

pub(super) fn source_kind_name(value: MemorySourceKind) -> String {
    match value {
        MemorySourceKind::Human => "human",
        MemorySourceKind::Agent => "agent",
        MemorySourceKind::Projection => "projection",
        MemorySourceKind::Derived => "derived",
        _ => "",
    }
    .to_string()
}

pub(super) fn proto_timestamp_to_sort_string(value: Option<Timestamp>) -> Option<String> {
    let value = value?;
    Some(format!(
        "unix:{:012}:{:09}",
        value.seconds + UNIX_SORT_OFFSET,
        value.nanos.max(0)
    ))
}

pub(super) fn timestamp_from_sort_or_rfc3339(value: Option<&str>) -> Option<Timestamp> {
    let value = value?;
    parse_unix_sort_timestamp(value).or_else(|| parse_basic_rfc3339(value))
}

fn parse_unix_sort_timestamp(value: &str) -> Option<Timestamp> {
    let suffix = value.strip_prefix("unix:")?;
    let (seconds, nanos) = suffix.split_once(':')?;
    Some(Timestamp {
        seconds: seconds.parse::<i64>().ok()? - UNIX_SORT_OFFSET,
        nanos: nanos.parse::<i32>().ok()?,
    })
}

fn parse_basic_rfc3339(value: &str) -> Option<Timestamp> {
    if value.len() < 20 || !value.ends_with('Z') {
        return None;
    }
    let year = value[0..4].parse::<i64>().ok()?;
    let month = value[5..7].parse::<u8>().ok()?;
    let day = value[8..10].parse::<u8>().ok()?;
    let hour = value[11..13].parse::<u8>().ok()?;
    let minute = value[14..16].parse::<u8>().ok()?;
    let second = value[17..19].parse::<u8>().ok()?;
    Timestamp::date_time(year, month, day, hour, minute, second).ok()
}
