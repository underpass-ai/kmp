//! What the rendered wake JSON carries, and what reading it back restores.

use super::json_paths::array_len;
use super::response_value::wake_value;
use super::test_support::{labels_fixture, typed_wake_fixture};
use super::typed_response::apply_wake_value;

#[test]
fn wake_value_carries_the_labels_and_apply_reads_them_back() {
    let mut response = typed_wake_fixture(2);
    response.labels = labels_fixture(3);

    let value = wake_value(&response);

    assert_eq!(array_len(&value, &["labels"]), 3);
    assert_eq!(value["labels"][0]["about"], "project:kmp");
    assert_eq!(value["labels"][0]["key"], "task");
    assert_eq!(value["labels"][0]["value"], "underpass-ai-kmp-000");
    assert_eq!(value["labels"][0]["entries"], 3);
    assert!(value["labels"][0]["last_observed_at"].is_string());
    let applied = apply_wake_value(typed_wake_fixture(2), &value);
    assert_eq!(applied.labels, response.labels);
}
