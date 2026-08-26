#!/usr/bin/env bash

# Source this file from a test script and call `run_cargo_test` with the same
# arguments that would normally be passed to `cargo test`. CI sets
# KMP_COLLECT_COVERAGE=true so that the *same* test execution emits profiling
# data; local runs remain plain cargo test invocations.

run_cargo_test() {
  if [[ "${KMP_COLLECT_COVERAGE:-false}" == "true" ]]; then
    cargo llvm-cov --no-report "$@"
  else
    cargo test "$@"
  fi
}
