#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUTPUT_DIR="${PAPER_OUTPUT_DIR:-${ROOT_DIR}/artifacts/paper-use-cases}"
SUMMARY_PATH="${OUTPUT_DIR}/summary.json"
FIGURES_MD_PATH="${OUTPUT_DIR}/results-figures.md"

if [[ ! -f "${SUMMARY_PATH}" ]]; then
  echo "paper summary not found at ${SUMMARY_PATH}" >&2
  exit 1
fi

jq -r '
  def short_use_case:
    if . == "uc1_failure_diagnosis_rehydration" then "UC1"
    elif . == "uc2_why_implementation_trace" then "UC2"
    elif . == "uc3_interrupted_handoff_resume" then "UC3"
    elif . == "uc4_constraint_reason_under_token_pressure" then "UC4"
    else .
    end;

  def short_variant:
    if . == "full_explanatory_with_detail" then "full"
    elif . == "full_explanatory_without_detail" then "no-detail"
    elif . == "structural_only_with_detail" then "structural"
    elif . == "detail_only_with_detail" then "detail-only"
    elif (. | startswith("full_explanatory_with_detail__budget_")) then
      "full@" + (. | split("__budget_")[1])
    elif (. | startswith("full_explanatory_with_detail__meso")) then
      "full-meso"
    elif (. | startswith("detail_only_with_detail__meso")) then
      "detail-only-meso"
    elif (. | startswith("structural_only_with_detail__meso")) then
      "structural-meso"
    elif (. | startswith("full_explanatory_without_detail__meso")) then
      "no-detail-meso"
    elif (. | startswith("structural_only_with_detail__budget_")) then
      "structural@" + (. | split("__budget_")[1])
    elif (. | startswith("full_explanatory_without_detail__budget_")) then
      "no-detail@" + (. | split("__budget_")[1])
    else .
    end;

  def short_label:
    "\(.use_case_id | short_use_case)-\(.variant_id | short_variant)";

  def numeric_list(key):
    map(. as $row | $row[key]) | join(", ");

  def label_list:
    map(short_label) | join(", ");

  def max_tokens:
    (map(.rendered_token_count) | max) + 10;

  def continuation_rows:
    map(select(.rehydration_point_hit != null));

  def retry_rows:
    map(select(.retry_success_hit != null));

  [
    "# Paper Use Case Figures",
    "",
    "Source: `artifacts/paper-use-cases/summary.json`",
    "",
    "Legend:",
    "- `full`: full explanatory relations with detail",
    "- `no-detail`: full explanatory relations without detail",
    "- `structural`: structural-only relations with detail",
    "- `detail-only`: structural relations with explanation injected into node detail",
    "- `*@96`: same relation/detail mode rendered with a 96-token budget",
    "- `*-meso`: same variant over a denser noisy graph",
    "",
    "## Figure 1. Causal Reconstruction Score By Variant",
    "",
    "```mermaid",
    "xychart-beta",
    "    title \"Causal Reconstruction Score by Use Case and Variant\"",
    "    x-axis [" + (label_list) + "]",
    "    y-axis \"score\" 0 --> 1",
    "    bar [" + (numeric_list("causal_reconstruction_score")) + "]",
    "```",
    "",
    "## Figure 2. Rendered Token Count By Variant",
    "",
    "```mermaid",
    "xychart-beta",
    "    title \"Rendered Token Count by Use Case and Variant\"",
    "    x-axis [" + (label_list) + "]",
    "    y-axis \"tokens\" 0 --> " + ((max_tokens | tostring)),
    "    bar [" + (numeric_list("rendered_token_count")) + "]",
    "```",
    "",
    "## Figure 3. Continuation-Point Recovery For Operational Cases",
    "",
    "```mermaid",
    "xychart-beta",
    "    title \"Continuation-Point Hit by Variant\"",
    "    x-axis [" + ((continuation_rows | map(short_label) | join(", "))) + "]",
    "    y-axis \"hit\" 0 --> 1",
    "    bar [" + ((continuation_rows | map(if .rehydration_point_hit == true then 1 else 0 end) | join(", "))) + "]",
    "```",
    "",
    "## Figure 4. Dominant-Reason Preservation",
    "",
    "```mermaid",
    "xychart-beta",
    "    title \"Dominant-Reason Hit by Variant\"",
    "    x-axis [" + (map(select(.dominant_reason_hit != null)) | map(short_label) | join(", ")) + "]",
    "    y-axis \"hit\" 0 --> 1",
    "    bar [" + (map(select(.dominant_reason_hit != null)) | map(if .dominant_reason_hit == true then 1 else 0 end) | join(", ")) + "]",
    "```",
    "",
    "## Figure 5. Closed-Loop Retry Success For Operational Cases",
    "",
    "```mermaid",
    "xychart-beta",
    "    title \"Retry Success Hit by Variant\"",
    "    x-axis [" + ((retry_rows | map(short_label) | join(", "))) + "]",
    "    y-axis \"hit\" 0 --> 1",
    "    bar [" + ((retry_rows | map(if .retry_success_hit == true then 1 else 0 end) | join(", "))) + "]",
    "```"
  ]
  | .[]
' "${SUMMARY_PATH}" > "${FIGURES_MD_PATH}"

printf 'paper figures written to %s\n' "${FIGURES_MD_PATH}"
