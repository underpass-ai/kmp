use std::time::SystemTime;

use prost_types::Timestamp;

pub(crate) fn timestamp_from(value: SystemTime) -> Timestamp {
    Timestamp::from(value)
}
