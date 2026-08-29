#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUTPUT_DIR="${PAPER_OUTPUT_DIR:-${ROOT_DIR}/artifacts/paper-use-cases}"
SUMMARY_PATH="${OUTPUT_DIR}/summary.json"
RESULTS_MD_PATH="${OUTPUT_DIR}/results.md"
RESULTS_CSV_PATH="${OUTPUT_DIR}/results.csv"

if [[ ! -f "${SUMMARY_PATH}" ]]; then
  echo "paper summary not found at ${SUMMARY_PATH}" >&2
  exit 1
fi

jq -r '
  ([
    "use_case_id",
    "variant_id",
    "relation_variant",
    "detail_variant",
    "graph_scale",
    "requested_token_budget",
    "bundle_nodes",
    "bundle_relationships",
    "detailed_nodes",
    "rendered_token_count",
    "explanation_roundtrip_fidelity",
    "detail_roundtrip_fidelity",
    "causal_reconstruction_score",
    "dominant_reason_hit",
    "rehydration_point_hit",
    "retry_success_hit",
    "retry_success_rate",
    "suspect_relationship_count"
  ] | @csv),
  (.[] | [
    .use_case_id,
    .variant_id,
    .relation_variant,
    .detail_variant,
    .graph_scale,
    .requested_token_budget,
    .bundle_nodes,
    .bundle_relationships,
    .detailed_nodes,
    .rendered_token_count,
    .explanation_roundtrip_fidelity,
    .detail_roundtrip_fidelity,
    .causal_reconstruction_score,
    (.dominant_reason_hit // ""),
    (.rehydration_point_hit // ""),
    (.retry_success_hit // ""),
    (.retry_success_rate // ""),
    (.suspect_relationship_count // "")
  ] | @csv)
' "${SUMMARY_PATH}" > "${RESULTS_CSV_PATH}"

jq -r '
  def row:
    "| \(.use_case_id) | \(.variant_id) | \(.relation_variant) | \(.detail_variant) | \(.graph_scale) | \(.requested_token_budget) | \(.explanation_roundtrip_fidelity) | \(.detail_roundtrip_fidelity) | \(.causal_reconstruction_score) | \(.retry_success_hit // "") | \(.retry_success_rate // "") | \(.rendered_token_count) |";

  [
    "# Paper Use Case Results",
    "",
    "Source: `artifacts/paper-use-cases/summary.json`",
    "",
    "## Metrics Table",
    "",
    "| Use case | Variant | Relation mode | Detail mode | Scale | Budget | Explanation fidelity | Detail fidelity | Causal score | Retry hit | Retry score | Tokens |",
    "| --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |"
  ]
  + (map(row))
  + [
    "",
    "## Key Findings",
    "",
    (
      .[]
      | "- \(.use_case_id): `\(.variant_id)` reaches explanation fidelity `\(.explanation_roundtrip_fidelity)`, causal score `\(.causal_reconstruction_score)`, retry score `\(.retry_success_rate // "n/a")`, and renders `\(.rendered_token_count)` tokens under budget `\(.requested_token_budget)`."
    )
  ]
  | .[]
' "${SUMMARY_PATH}" > "${RESULTS_MD_PATH}"

printf 'paper report written to %s and %s\n' "${RESULTS_MD_PATH}" "${RESULTS_CSV_PATH}"
