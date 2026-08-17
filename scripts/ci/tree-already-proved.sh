#!/usr/bin/env bash
# Was this exact working tree already proved green?
#
# A merge to main usually lands a tree byte-identical to the pull request head
# that was just tested: same tree hash, different commit sha. The gates
# compile and test a working tree, so running them again proves nothing —
# while doubling the exposure to intermittent container failures (#30), and
# every false red trains the reflex to re-run rather than read.
#
# The rule is "skip when the tree was already proved", never "trust the pull
# request". A merge from an out-of-date branch, a conflict resolved in the UI
# and a direct push all produce trees nobody tested, and each gets the full
# gate. Any doubt — an API that will not answer, a run whose commit we cannot
# resolve — runs the gates.
set -uo pipefail

WORKFLOW="${1:-quality-gate.yml}"
RUNS_TO_CHECK="${TREE_PROOF_RUNS:-30}"

say() { printf '%s\n' "$*" >&2; }
answer() { printf 'skip=%s\n' "$1" >>"${GITHUB_OUTPUT:-/dev/stdout}"; exit 0; }

TREE="$(git rev-parse HEAD^{tree} 2>/dev/null)"
if [ -z "${TREE}" ]; then
  say "cannot resolve this tree; running the gates"
  answer false
fi
say "this tree: ${TREE}"

REPO="${GITHUB_REPOSITORY:-$(gh repo view --json nameWithOwner -q .nameWithOwner 2>/dev/null)}"
if [ -z "${REPO}" ]; then
  say "cannot resolve the repository; running the gates"
  answer false
fi

# Successful runs of this same workflow, newest first. A run proves the tree
# of the commit it ran on, whatever branch that commit was on.
SHAS="$(gh api "repos/${REPO}/actions/workflows/${WORKFLOW}/runs?status=success&per_page=${RUNS_TO_CHECK}" \
  --jq '.workflow_runs[].head_sha' 2>/dev/null)"
if [ -z "${SHAS}" ]; then
  say "no successful runs to compare against; running the gates"
  answer false
fi

for sha in ${SHAS}; do
  [ "${sha}" = "${GITHUB_SHA:-}" ] && continue
  proved="$(gh api "repos/${REPO}/commits/${sha}" --jq '.commit.tree.sha' 2>/dev/null)"
  if [ "${proved}" = "${TREE}" ]; then
    say "tree already proved green by ${sha}"
    answer true
  fi
done

say "no green run covers this tree; running the gates"
answer false
